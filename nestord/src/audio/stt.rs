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

/// Lettres accentuees et ligatures du francais.
const FRENCH_LETTERS: &str = "àâäçéèêëîïôöùûüÿœæÀÂÄÇÉÈÊËÎÏÔÖÙÛÜŸŒÆ";

/// La transcription ressemble-t-elle a une hallucination de Whisper plutot qu'a une phrase ?
///
/// Sur un son qu'il ne comprend pas (bruit, audio abime), le modele peut deriver vers une
/// autre langue ou boucler sur quelques mots, meme avec la langue forcee. Deux signes
/// suffisent a l'ecarter sans risque pour une vraie phrase :
/// - en francais, des lettres d'un autre alphabet (« þ », « ð », cyrillique...) ;
/// - un meme enchainement de mots repete en boucle.
///
/// Retourne la raison, ou `None` si la transcription est plausible.
pub fn hallucination_reason(text: &str, language: &str) -> Option<&'static str> {
    if language == "fr" {
        let foreign = text.chars().filter(|c| c.is_alphabetic() && !c.is_ascii() && !FRENCH_LETTERS.contains(*c)).count();
        if foreign >= 2 {
            return Some("lettres etrangeres au francais");
        }
    }

    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    if words.len() >= 8 {
        let mut counts: std::collections::HashMap<(&str, &str), usize> = std::collections::HashMap::new();
        for pair in words.windows(2) {
            *counts.entry((pair[0].as_str(), pair[1].as_str())).or_default() += 1;
        }
        let top = counts.values().copied().max().unwrap_or(0);
        // Un meme couple de mots au moins 4 fois, et sur plus du tiers de l'enonce.
        if top >= 4 && top * 3 >= words.len() {
            return Some("meme suite de mots repetee en boucle");
        }
    }
    None
}

/// Enregistre un enonce en WAV (16 kHz mono) pour le diagnostic. Active par
/// `NESTORD_DEBUG_AUDIO_DIR` ; ces fichiers contiennent la voix de l'utilisateur.
pub fn dump_wav(dir: &Path, samples: &[f32]) -> Result<std::path::PathBuf> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = dir.join(format!("enonce-{millis}.wav"));
    let data_len = (samples.len() * 2) as u32;
    let mut file = std::io::BufWriter::new(std::fs::File::create(&path)?);
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&1u16.to_le_bytes())?; // mono
    file.write_all(&16_000u32.to_le_bytes())?;
    file.write_all(&32_000u32.to_le_bytes())?; // octets par seconde
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        file.write_all(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hallucinations_ecartees() {
        // Les deux transcriptions relevees en session reelle.
        assert!(hallucination_reason("Búnusar, reis stöðu að hó.", "fr").is_some());
        let loop_text = "Hvað er þetta er það? ".to_string() + &"Hvað er það? ".repeat(20);
        assert!(hallucination_reason(&loop_text, "fr").is_some());
        // Boucle sans lettre etrangere.
        assert!(hallucination_reason(&"merci beaucoup ".repeat(8), "fr").is_some());
    }

    #[test]
    fn vraies_phrases_conservees() {
        for ok in [
            "Bonjour, Nestor. Est-ce que cette fois-ci tu comptes en français ?",
            "Wow!",
            "Où est le fichier de configuration, s'il te plaît ?",
            "Non, non, non, ce n'est pas ça que je voulais dire.",
            "Lance les tests, puis relance les tests si les tests échouent.",
            "Écris à Jürgen Müller.",
        ] {
            assert_eq!(hallucination_reason(ok, "fr"), None, "{ok}");
        }
        // Dans une autre langue, seules les boucles sont ecartees.
        assert_eq!(hallucination_reason("Hvað er þetta?", "is"), None);
    }
}
