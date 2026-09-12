//! Text-to-speech via Piper (VITS, ONNX), phonemisation par `espeak-ng` (IPA).
//!
//! Piper est prefere a Kokoro-82M ici car il propose de vraies voix masculines
//! francaises (Kokoro v1.0 n'embarque qu'une seule voix fr, feminine).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use ort::ep::CUDA;
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;

/// Configuration `.onnx.json` accompagnant chaque voix Piper (champs utiles).
#[derive(Debug, Deserialize)]
struct PiperConfig {
    audio: AudioConfig,
    inference: InferenceConfig,
    espeak: EspeakConfig,
    phoneme_id_map: HashMap<String, Vec<i64>>,
    #[serde(default = "default_num_speakers")]
    num_speakers: u32,
}

#[derive(Debug, Deserialize)]
struct AudioConfig {
    sample_rate: u32,
}

#[derive(Debug, Deserialize)]
struct InferenceConfig {
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
}

#[derive(Debug, Deserialize)]
struct EspeakConfig {
    voice: String,
}

fn default_num_speakers() -> u32 {
    1
}

pub struct PiperTts {
    session: Mutex<Session>,
    config: PiperConfig,
    speaker_id: i64,
}

impl PiperTts {
    /// Charge la voix `voice_name` depuis `piper_dir` (attend
    /// `<voice_name>.onnx` et `<voice_name>.onnx.json`). `speaker_id` n'est
    /// utilise que pour les modeles multi-locuteurs.
    pub fn load(piper_dir: &Path, voice_name: &str, speaker_id: i64) -> Result<Self> {
        let model_path = piper_dir.join(format!("{voice_name}.onnx"));
        let config_path = piper_dir.join(format!("{voice_name}.onnx.json"));

        let raw_config = std::fs::read_to_string(&config_path)
            .with_context(|| format!("lecture de la config {}", config_path.display()))?;
        let config: PiperConfig = serde_json::from_str(&raw_config)
            .with_context(|| format!("parsing de la config {}", config_path.display()))?;

        anyhow::ensure!(
            speaker_id == 0 || (speaker_id as u32) < config.num_speakers,
            "locuteur {speaker_id} inexistant pour la voix '{voice_name}' ({} locuteur(s))",
            config.num_speakers
        );

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("creation du builder ONNX Runtime: {e}"))?
            .with_execution_providers([CUDA::default().build()])
            .map_err(|e| anyhow::anyhow!("activation de l'EP CUDA pour Piper: {e}"))?
            .commit_from_file(&model_path)
            .with_context(|| format!("chargement du modele Piper {}", model_path.display()))?;

        tracing::info!(
            voice = voice_name,
            speaker_id,
            sample_rate = config.audio.sample_rate,
            "voix Piper chargee"
        );

        Ok(Self { session: Mutex::new(session), config, speaker_id })
    }

    /// Frequence d'echantillonnage de la voix chargee (annoncee au front dans
    /// l'evenement `audio_chunk`).
    pub fn sample_rate(&self) -> u32 {
        self.config.audio.sample_rate
    }

    /// Synthetise `text` et retourne un buffer mono f32.
    pub fn synthesize(&self, text: &str) -> Result<Vec<f32>> {
        let phonemes = phonemize(text, &self.config.espeak.voice)?;
        anyhow::ensure!(!phonemes.is_empty(), "aucun phoneme produit pour le texte fourni");

        let ids = self.phonemes_to_ids(&phonemes)?;

        let input = Tensor::from_array((vec![1i64, ids.len() as i64], ids.clone()))?;
        let input_lengths = Tensor::from_array((vec![1i64], vec![ids.len() as i64]))?;
        let scales = Tensor::from_array((
            vec![3i64],
            vec![
                self.config.inference.noise_scale,
                self.config.inference.length_scale,
                self.config.inference.noise_w,
            ],
        ))?;
        let sid = Tensor::from_array((vec![1i64], vec![self.speaker_id]))?;

        let mut session = self.session.lock().expect("verrou de session Piper empoisonne");
        let outputs = session.run(ort::inputs![
            "input" => input,
            "input_lengths" => input_lengths,
            "scales" => scales,
            "sid" => sid,
        ])?;

        let (_, waveform) = outputs["output"].try_extract_tensor::<f32>()?;
        anyhow::ensure!(
            waveform.iter().all(|s| s.is_finite()),
            "Piper a produit une forme d'onde invalide (NaN)"
        );

        Ok(waveform.to_vec())
    }

    /// Convertit une chaine de phonemes IPA en identifiants attendus par le
    /// modele : `^` (debut), chaque phoneme suivi du remplissage `_`, puis `$`.
    fn phonemes_to_ids(&self, phonemes: &str) -> Result<Vec<i64>> {
        let map = &self.config.phoneme_id_map;
        let pad = map.get("_").and_then(|ids| ids.first().copied()).context("phoneme_id_map sans '_'")?;

        let mut ids = map.get("^").context("phoneme_id_map sans '^'")?.clone();
        for ch in phonemes.chars() {
            match map.get(&ch.to_string()) {
                Some(phoneme_ids) => {
                    ids.extend_from_slice(phoneme_ids);
                    ids.push(pad);
                }
                None => tracing::debug!(symbole = %ch, "symbole IPA hors vocabulaire Piper, ignore"),
            }
        }
        ids.extend_from_slice(map.get("$").context("phoneme_id_map sans '$'")?);

        Ok(ids)
    }
}

/// Phonemise un texte via `espeak-ng --ipa`, puis aplatit le resultat en une
/// chaine de symboles IPA sans separateur de phonemes.
fn phonemize(text: &str, espeak_voice: &str) -> Result<String> {
    let output = Command::new("espeak-ng")
        .args(["--ipa=1", "-q", "-v", espeak_voice, text])
        .output()
        .context("execution d'espeak-ng impossible (paquet installe ?)")?;

    if !output.status.success() {
        bail!("espeak-ng a echoue: {}", String::from_utf8_lossy(&output.stderr));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let without_markers = strip_language_markers(&raw);
    let words: Vec<&str> = without_markers.split_whitespace().collect();
    Ok(words.join(" ").replace('_', ""))
}

/// Retire les marqueurs de changement de langue d'espeak-ng (`(en)`, `(fr)`).
///
/// espeak-ng bascule automatiquement de langue sur les mots etrangers qu'il
/// reconnait (utile : les mots anglais sont alors phonemises en anglais) et
/// signale la bascule par ces marqueurs. Sans ce filtrage, leurs caracteres
/// sont mappes comme des phonemes et la synthese prononce litteralement le
/// code de langue au milieu de la phrase.
fn strip_language_markers(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth = 0usize;
    for ch in raw.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}
