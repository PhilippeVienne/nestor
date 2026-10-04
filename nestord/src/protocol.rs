//! Contrat JSON echange avec l'UI Antigravity sur `ws://127.0.0.1:8340/ws`.
//! Toute modification ici doit rester synchronisee avec `.agents/INITIAL-AGY.md`.

use serde::{Deserialize, Serialize};

/// Evenements emis par le daemon vers l'UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    State {
        status: DaemonStatus,
    },
    /// Amplitude du flux micro, pour l'orbe reactif de l'UI. Pas encore emis
    /// par `audio/mod.rs` (VAD/RMS a cabler cote pipeline d'entree).
    #[allow(dead_code)]
    AudioLevels {
        rms: f32,
        peak: f32,
    },
    Transcript {
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<String>,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_final: Option<bool>,
    },
    ToolCall {
        name: String,
        input: serde_json::Value,
        status: ToolCallStatus,
        /// Present quand l'outil est execute par un sous-agent de mission et
        /// non par la session conversationnelle.
        #[serde(skip_serializing_if = "Option::is_none")]
        mission_id: Option<u64>,
    },
    /// Cycle de vie d'une mission deleguee a un sous-agent.
    Mission {
        id: u64,
        backend: String,
        status: MissionStatus,
        description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        /// Derniere activite observee du sous-agent (outil en cours, phrase en
        /// cours de redaction). Permet de suivre l'avancement plutot que de
        /// n'avoir que « en cours » jusqu'au compte rendu final.
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<String>,
    },
    /// Consommation du quota de la session Claude locale, telle que rapportee
    /// par le CLI. Sert au routage des missions et a l'affichage cote UI.
    Usage {
        five_hour: f32,
        seven_day: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        resets_at: Option<u64>,
    },
    /// Segment audio TTS synthetise, encode en base64. Le front (Antigravity)
    /// decode `data` et lit un PCM16 mono brut a `sample_rate` Hz quand
    /// `format == "pcm16"` (evite l'ambiguite avec les frames binaires brutes,
    /// que le front interprete toujours comme du WAV).
    #[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
    AudioChunk {
        data: String,
        format: String,
        sample_rate: u32,
    },
    /// Etat du moteur conversationnel de Nestor (Claude vs AGY en mode reduit).
    BackendStatus {
        active_backend: String,
        is_fallback: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Etat du mot-cle d'activation : en veille (attend "Hey Nestor") ou actif (en dialogue).
    WakeState {
        active: bool,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonStatus {
    Listening,
    Thinking,
    Speaking,
    Idle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Running,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// Evenements recus par le daemon depuis l'UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    #[serde(alias = "interrupt")]
    BargeIn,
    #[serde(alias = "chat_message")]
    SendText {
        #[serde(alias = "text")]
        content: String,
    },
    /// Envoye par le front en plus de la frame binaire equivalente (meme
    /// audio, deux vehicules). On l'accepte pour ne pas logguer une erreur de
    /// parsing, mais on l'ignore : le canal binaire est deja traite.
    AudioIn {
        #[allow(dead_code)]
        pcm_base64: String,
        #[allow(dead_code)]
        sample_rate: u32,
    },
    /// Annulation d'une mission en cours, declenchee depuis l'UI.
    StopMission {
        id: u64,
        /// Motif facultatif, repris dans le compte rendu d'annulation.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Bascule manuelle du backend conversationnel ("claude", "agy", ou "auto").
    SetBackend {
        backend: String,
    },
    /// Position GPS courante, envoyee periodiquement par le front pour
    /// alimenter la reconnaissance de lieu (`Config::place_at`).
    Location {
        lat: f64,
        lon: f64,
    },
}
