//! Speech-to-text via Whisper large-v3-turbo (`whisper-rs`, execution CUDA).

use std::path::Path;

use anyhow::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Duree minimale d'audio soumise a Whisper. Le modele travaille sur des
/// fenetres de 30 s et se degrade fortement sur des extraits tres courts :
/// on complete au silence plutot que de lui donner 300 ms brutes.
const MIN_AUDIO_SAMPLES: usize = 16_000; // 1 s a 16 kHz

/// Amorce de vocabulaire : oriente le decodage vers le domaine de Nestor.
/// Sans elle, les termes techniques anglais sont rendus par des mots francais
/// proches phonetiquement ("Bash" -> « bâches », "tool use" -> « Toulouse »).
const INITIAL_PROMPT: &str =
    "Conversation technique en francais avec Nestor, assistant de developpement. \
Termes employes : Bash, tool use, tool call, Claude, Claude Code, Antigravity, agy, \
Nestor, nestord, MCP, WebSocket, daemon, Rust, Cargo, crate, TypeScript, React, Vite, \
Whisper, Silero, Piper, Kokoro, VAD, STT, TTS, PCM, base64, commit, build, branche, \
merge, refactor, log, buffer, thread, front, backend, mission, sous-agent, quota, \
prompt, token, fichier, dossier, test, deploiement.";

/// Vocabulaire additionnel du projet, une entree par ligne (les lignes vides et
/// celles commencant par `#` sont ignorees). Permet d'ajouter des noms propres
/// ou du jargon maison sans recompiler.
const VOCAB_FILE_NAME: &str = "vocabulaire.txt";

pub struct WhisperStt {
    ctx: WhisperContext,
    prompt: String,
}

impl WhisperStt {
    pub fn load(model_path: &Path) -> Result<Self> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu(true);

        let ctx = WhisperContext::new_with_params(model_path, params)
            .with_context(|| format!("chargement du modele Whisper {}", model_path.display()))?;

        // Le fichier de vocabulaire vit a cote des modeles : l'utilisateur peut
        // l'enrichir a chaud (au prochain demarrage) sans toucher au code.
        let vocab_path = model_path.parent().unwrap_or(Path::new(".")).join(VOCAB_FILE_NAME);
        let prompt = build_prompt(&vocab_path);

        Ok(Self { ctx, prompt })
    }

    /// Transcrit un buffer mono f32 a 16 kHz. `language` accepte un code ISO
    /// ("fr", "en", ...) ou "auto" pour laisser Whisper detecter la langue.
    ///
    /// La detection automatique est deconseillee ici : sur des enonces courts,
    /// Whisper identifie regulierement du francais comme de l'anglais et
    /// produit alors une phrase anglaise inventee.
    pub fn transcribe(&self, samples: &[f32], language: &str) -> Result<String> {
        let mut state = self.ctx.create_state().context("creation de l'etat whisper")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        if language == "auto" {
            params.set_detect_language(true);
        } else {
            params.set_language(Some(language));
        }

        // Chaque enonce est independant : sans ca, Whisper enchaine sur le
        // contexte precedent et derive (repetitions, phrases inventees).
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_suppress_blank(true);
        // Pas de non-speech tokens (bruits de bouche, "(musique)", etc.).
        params.set_suppress_nst(true);
        params.set_no_timestamps(true);
        // Decodage deterministe : la remontee de temperature invente du texte
        // quand le signal est pauvre.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);
        params.set_no_speech_thold(0.6);
        params.set_initial_prompt(&self.prompt);

        let padded = pad_to_minimum(samples);
        state.full(params, &padded).context("inference whisper")?;

        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(s) = segment.to_str_lossy() {
                    text.push_str(&s);
                }
            }
        }

        Ok(text.trim().to_string())
    }
}

/// Concatene l'amorce integree et le vocabulaire utilisateur eventuel.
fn build_prompt(vocab_path: &Path) -> String {
    let extra: Vec<String> = match std::fs::read_to_string(vocab_path) {
        Ok(content) => content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    };

    if extra.is_empty() {
        return INITIAL_PROMPT.to_string();
    }

    tracing::info!(
        count = extra.len(),
        path = %vocab_path.display(),
        "vocabulaire supplementaire charge pour la transcription"
    );
    format!("{INITIAL_PROMPT} Autres termes : {}.", extra.join(", "))
}

fn pad_to_minimum(samples: &[f32]) -> Vec<f32> {
    if samples.len() >= MIN_AUDIO_SAMPLES {
        return samples.to_vec();
    }
    let mut padded = Vec::with_capacity(MIN_AUDIO_SAMPLES);
    padded.extend_from_slice(samples);
    padded.resize(MIN_AUDIO_SAMPLES, 0.0);
    padded
}
