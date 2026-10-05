//! Reglages modifiables a chaud depuis l'UI (ecran « Reglages »).
//!
//! `Config` (config.toml + variables d'environnement) reste la source au
//! demarrage et n'est jamais reecrite : un fichier ecrit a la main garderait
//! mal ses commentaires. Les changements faits dans l'UI sont enregistres a
//! part, dans `~/.config/nestord/ui-settings.toml`, et relus au demarrage.
//!
//! Priorite au demarrage : config.toml < ui-settings.toml < variables
//! d'environnement. Une fois le daemon lance, l'UI a le dernier mot.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::config::{Config, JudgeConfig};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Parler par-dessus Nestor coupe sa reponse.
    pub voice_barge_in: bool,
    /// Annulation d'echo cote serveur pendant la lecture.
    pub aec: bool,
    /// Probabilite de parole (VAD) exigee par fenetre pour une interruption.
    pub barge_threshold: f32,
    /// Duree de parole exigee avant d'interrompre.
    pub barge_min_speech_ms: u64,
    /// Energie minimale (RMS, apres AEC) d'une fenetre de parole.
    pub barge_min_rms: f32,
    /// Fin de tour par modele (Smart Turn) au lieu du silence fixe.
    pub smart_turn: bool,
    /// Exiger le mot-cle en veille.
    pub wake_word_enabled: bool,
    /// Fenetre de dialogue sans mot-cle apres chaque echange.
    pub wake_timeout_secs: u64,
    pub judge_model: String,
    pub judge_confirm_threshold: u8,
    pub judge_reject_threshold: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl Settings {
    pub fn from_config(config: &Config) -> Self {
        Self {
            voice_barge_in: config.barge_in.voice,
            aec: config.barge_in.aec,
            barge_threshold: config.barge_in.threshold,
            barge_min_speech_ms: config.barge_in.min_speech_ms,
            barge_min_rms: config.barge_in.min_rms,
            smart_turn: config.turn.enabled,
            wake_word_enabled: config.wake_word.enabled,
            wake_timeout_secs: config.wake_word.timeout_secs,
            judge_model: config.judge.model.clone(),
            judge_confirm_threshold: config.judge.confirm_threshold,
            judge_reject_threshold: config.judge.reject_threshold,
        }
    }

    /// Borne les valeurs recues de l'UI : un reglage aberrant ne doit ni
    /// rendre Nestor sourd ni desactiver le juge par un seuil incoherent.
    pub fn sanitized(mut self) -> Self {
        self.barge_threshold = if self.barge_threshold.is_finite() { self.barge_threshold.clamp(0.3, 0.99) } else { 0.75 };
        self.barge_min_speech_ms = self.barge_min_speech_ms.clamp(100, 2000);
        self.barge_min_rms = if self.barge_min_rms.is_finite() { self.barge_min_rms.clamp(0.0, 0.2) } else { 0.012 };
        self.wake_timeout_secs = self.wake_timeout_secs.clamp(3, 300);
        self.judge_model = self.judge_model.trim().to_string();
        if self.judge_model.is_empty() {
            self.judge_model = JudgeConfig::default().model;
        }
        self.judge_reject_threshold = self.judge_reject_threshold.clamp(1, 100);
        self.judge_confirm_threshold = self.judge_confirm_threshold.clamp(1, self.judge_reject_threshold);
        self
    }

    /// Configuration du juge avec le modele et les seuils regles dans l'UI.
    pub fn judge_config(&self, base: &JudgeConfig) -> JudgeConfig {
        JudgeConfig {
            model: self.judge_model.clone(),
            confirm_threshold: self.judge_confirm_threshold,
            reject_threshold: self.judge_reject_threshold,
            ..base.clone()
        }
    }
}

struct Live {
    current: RwLock<Settings>,
    version: AtomicU64,
    path: PathBuf,
}

static LIVE: OnceLock<Live> = OnceLock::new();

fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/nestord/ui-settings.toml")
}

fn env_is_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.trim().is_empty())
}

/// Initialise les reglages a partir de la configuration chargee, puis applique
/// ceux enregistres par l'UI. A appeler une fois au demarrage.
pub fn init(config: &Config) {
    let from_config = Settings::from_config(config);
    let path = settings_path();
    let mut settings = from_config.clone();

    if let Ok(raw) = std::fs::read_to_string(&path) {
        match toml::from_str::<Settings>(&raw) {
            Ok(saved) => {
                settings = saved;
                // Une variable d'environnement explicite garde la main au demarrage.
                if env_is_set("NESTORD_VOICE_BARGE_IN") {
                    settings.voice_barge_in = from_config.voice_barge_in;
                }
                if env_is_set("NESTORD_AEC") {
                    settings.aec = from_config.aec;
                }
                if env_is_set("NESTORD_TURN_DETECTION") {
                    settings.smart_turn = from_config.smart_turn;
                }
                if env_is_set("NESTORD_WAKE_WORD_ENABLED") {
                    settings.wake_word_enabled = from_config.wake_word_enabled;
                }
                if env_is_set("NESTORD_WAKE_TIMEOUT_SECS") {
                    settings.wake_timeout_secs = from_config.wake_timeout_secs;
                }
                tracing::info!(path = %path.display(), "reglages de l'UI charges");
            }
            Err(err) => tracing::warn!(?err, path = %path.display(), "reglages de l'UI illisibles, ignores"),
        }
    }

    let _ = LIVE.set(Live { current: RwLock::new(settings.sanitized()), version: AtomicU64::new(1), path });
}

/// Reglages courants. Avant `init` (tests), ceux de la configuration par defaut.
pub fn get() -> Settings {
    match LIVE.get() {
        Some(live) => live.current.read().unwrap().clone(),
        None => Settings::default(),
    }
}

/// Numero de version, incremente a chaque changement : permet aux boucles
/// temps reel de ne recopier les reglages que lorsqu'ils ont change.
#[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
pub fn version() -> u64 {
    LIVE.get().map(|live| live.version.load(Ordering::SeqCst)).unwrap_or(0)
}

/// Applique et enregistre de nouveaux reglages ; retourne ceux retenus apres bornage.
pub fn update(new: Settings) -> Settings {
    let new = new.sanitized();
    let Some(live) = LIVE.get() else { return new };
    {
        let mut guard = live.current.write().unwrap();
        if *guard == new {
            return new;
        }
        *guard = new.clone();
    }
    live.version.fetch_add(1, Ordering::SeqCst);

    let saved = toml::to_string_pretty(&new).map_err(anyhow::Error::from).and_then(|text| {
        if let Some(dir) = live.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&live.path, text)?;
        Ok(())
    });
    match saved {
        Ok(()) => tracing::info!(path = %live.path.display(), "reglages de l'UI enregistres"),
        Err(err) => tracing::warn!(?err, "reglages appliques mais non enregistres"),
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_aberrantes_bornees() {
        let s = Settings {
            barge_threshold: 5.0,
            barge_min_speech_ms: 0,
            barge_min_rms: f32::NAN,
            wake_timeout_secs: 0,
            judge_model: "   ".to_string(),
            judge_confirm_threshold: 95,
            judge_reject_threshold: 80,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.barge_threshold, 0.99);
        assert_eq!(s.barge_min_speech_ms, 100);
        assert_eq!(s.barge_min_rms, 0.012);
        assert_eq!(s.wake_timeout_secs, 3);
        assert_eq!(s.judge_model, JudgeConfig::default().model);
        // Le seuil de confirmation ne peut pas depasser celui de refus.
        assert_eq!((s.judge_confirm_threshold, s.judge_reject_threshold), (80, 80));
    }

    #[test]
    fn aller_retour_toml_et_champs_manquants() {
        let s = Settings { smart_turn: true, barge_threshold: 0.6, ..Settings::default() };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(s, back);
        // Un fichier partiel (ancienne version) garde les valeurs par defaut ailleurs.
        let partial: Settings = toml::from_str("smart_turn = true\n").unwrap();
        assert!(partial.smart_turn);
        assert_eq!(partial.barge_threshold, Settings::default().barge_threshold);
    }

    #[test]
    fn juge_reprend_le_modele_regle() {
        let s = Settings { judge_model: "llama3.2:3b".to_string(), judge_confirm_threshold: 50, ..Settings::default() };
        let j = s.judge_config(&JudgeConfig::default());
        assert_eq!(j.model, "llama3.2:3b");
        assert_eq!(j.confirm_threshold, 50);
        assert_eq!(j.ollama_host, JudgeConfig::default().ollama_host);
    }
}
