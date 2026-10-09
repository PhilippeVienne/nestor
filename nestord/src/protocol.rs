//! Contrat JSON echange avec l'UI web et l'appli mobile sur `ws://127.0.0.1:8340/ws`.
//! Toute modification ici doit rester synchronisee avec `web/src/types.ts` et `mobile/src/native/NestorCall.ts`.

use serde::{Deserialize, Serialize};

/// Evenements emis par le daemon vers l'UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Nestor vient d'etre interrompu a la voix : les clients doivent couper
    /// immediatement leur lecture audio en cours.
    #[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
    Interrupt {
        /// Niveau moyen (RMS) de la parole qui a declenche l'interruption.
        #[serde(skip_serializing_if = "Option::is_none")]
        rms: Option<f32>,
    },
    /// Enonce transcrit puis ecarte car il recopiait la voix de Nestor (echo).
    #[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
    EchoDiscarded {
        text: String,
    },
    /// Decision du juge de conscience sur un message ou une mission (panneau « Conscience »).
    /// `pending` : une confirmation de l'utilisateur est attendue (voix ou bouton).
    JudgeVerdict {
        id: u64,
        /// "message" (demande de l'utilisateur) ou "mission" (delegation a un sous-agent).
        source: String,
        text: String,
        /// "allow", "confirm" ou "deny".
        decision: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
        pending: bool,
        /// Horodatage de la decision (epoch ms).
        at_ms: u64,
    },
    /// Une confirmation en attente a ete tranchee (approuvee, refusee ou abandonnee).
    JudgeResolved {
        id: u64,
        approved: bool,
    },
    /// Contexte tenu par nestord (panneau « Situation »).
    Context {
        /// Lieu reconnu a partir de la position du client, s'il y en a un.
        #[serde(skip_serializing_if = "Option::is_none")]
        place: Option<String>,
        quiet_start: String,
        quiet_end: String,
        quiet_active: bool,
        /// Un jeton est exige pour se connecter a `/ws`.
        auth_required: bool,
    },
    /// Taches et rappels en attente, rediffuses a chaque changement.
    Todos {
        items: Vec<crate::todo::Todo>,
    },
    /// Clients connectes au daemon (web, mobile...).
    Clients {
        items: Vec<crate::dashboard::ClientInfo>,
    },
    /// Modeles charges, carte graphique et dernieres latences mesurees.
    Telemetry {
        #[serde(skip_serializing_if = "Option::is_none")]
        stt_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tts_voice: Option<String>,
        judge_model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        gpu: Option<crate::dashboard::GpuInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stt_ms: Option<u64>,
        /// Delai entre le debut de reflexion et le premier mot de la reponse.
        #[serde(skip_serializing_if = "Option::is_none")]
        first_word_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tts_ms: Option<u64>,
    },
    /// Serveurs MCP exposes a l'assistant (panneau « Connecteurs »).
    Connectors {
        items: Vec<crate::connectors::ConnectorInfo>,
    },
    /// Un outil externe en ecriture attend l'accord de l'utilisateur.
    ToolApproval {
        id: u64,
        server: String,
        tool: String,
        /// Arguments de l'appel (JSON lisible, tronque).
        arguments: String,
        at_ms: u64,
    },
    /// La confirmation d'ecriture a ete tranchee (accord, refus ou delai depasse).
    ToolApprovalResolved {
        id: u64,
        approved: bool,
    },
    /// Transcription ecartee car manifestement incoherente (hallucination de Whisper).
    #[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
    TranscriptRejected {
        reason: String,
    },
    /// Reglages courants (ecran « Reglages ») : a la connexion et a chaque changement.
    Settings {
        settings: crate::settings::Settings,
    },
    State {
        status: DaemonStatus,
    },
    /// Niveau du flux micro et probabilite de parole (VAD), ~10 fois par seconde.
    #[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
    AudioLevels {
        rms: f32,
        peak: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        vad: Option<f32>,
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
    /// Segment audio TTS synthetise, encode en base64. Le front
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
    /// Taches gerees depuis l'UI (memes operations que les outils MCP `todo_*`).
    TodoAdd {
        title: String,
        /// Echeance `AAAA-MM-JJTHH:MM` (tache ponctuelle).
        #[serde(default)]
        due_at: Option<String>,
        /// `daily`, `weekly:<jour>` ou `monthly:<1-31>`.
        #[serde(default)]
        recurrence: Option<String>,
    },
    TodoComplete {
        id: i64,
    },
    TodoDelete {
        id: i64,
    },
    /// Mode d'un outil externe choisi dans l'UI : lecture libre, confirmation ou non expose.
    SetToolMode {
        server: String,
        tool: String,
        mode: crate::connectors::ToolMode,
    },
    /// Accord ou refus, depuis l'UI, d'une ecriture externe en attente.
    ResolveToolApproval {
        id: u64,
        approve: bool,
    },
    /// Reponse de l'utilisateur, depuis l'UI, a une confirmation demandee par le juge.
    ResolveJudgement {
        id: u64,
        approve: bool,
    },
    /// Nouveaux reglages choisis dans l'UI (objet complet).
    UpdateSettings {
        settings: crate::settings::Settings,
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
