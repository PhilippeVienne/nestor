//! Donnees du tableau de bord : ce que l'UI affiche en plus du dialogue.
//!
//! - **Contexte** : lieu reconnu, heures calmes, jeton d'acces exige ou non.
//! - **Taches** : la liste tenue par `todo.rs`, rediffusee a chaque changement.
//! - **Appareils** : les clients WebSocket connectes (web, mobile...).
//! - **Telemetrie** : modeles charges, carte graphique, latences mesurees.
//! - **Connecteurs** : serveurs MCP exposes a l'assistant.
//!
//! Tout est diffuse sur le canal d'evenements commun ; un client qui se
//! connecte recoit un instantane (`ws::connection_snapshot`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::protocol::{DaemonStatus, Role, ServerEvent};
use crate::todo::TodoStore;
use crate::clock::now_ms;


// ---------------------------------------------------------------- appareils

#[derive(Debug, Clone, Serialize)]
pub struct ClientInfo {
    pub id: u64,
    /// "web", "mobile" ou "autre".
    pub kind: String,
    pub connected_at_ms: u64,
}

static CLIENTS: Mutex<Vec<ClientInfo>> = Mutex::new(Vec::new());
static CLIENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Devine le type de client : parametre `?client=` explicite, sinon l'en-tete User-Agent.
pub fn classify_client(explicit: Option<&str>, user_agent: Option<&str>) -> &'static str {
    match explicit.map(str::to_ascii_lowercase).as_deref() {
        Some("web") => return "web",
        Some("mobile") => return "mobile",
        // Telephone en veille : canal hors appel, sans audio (cf. ws.rs).
        Some("standby") => return "standby",
        _ => {}
    }
    let ua = user_agent.unwrap_or_default().to_ascii_lowercase();
    if ua.contains("okhttp") || ua.contains("android") {
        "mobile"
    } else if ua.contains("mozilla") {
        "web"
    } else {
        "autre"
    }
}

pub fn register_client(kind: &str) -> u64 {
    let id = CLIENT_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    CLIENTS.lock().unwrap().push(ClientInfo { id, kind: kind.to_string(), connected_at_ms: now_ms() });
    id
}

pub fn unregister_client(id: u64) {
    CLIENTS.lock().unwrap().retain(|c| c.id != id);
}

/// Clients connectes a `/ws`, tous types confondus.
pub fn client_count() -> usize {
    CLIENTS.lock().unwrap().len()
}

/// Clients qui entendent Nestor (web, appel mobile) : un telephone en veille ne
/// recoit que des notifications, pas la parole.
pub fn listening_count() -> usize {
    CLIENTS.lock().unwrap().iter().filter(|c| c.kind != "standby").count()
}

/// Clients connectes d'un type donne (`web`, `mobile`...).
pub fn client_count_of(kind: &str) -> usize {
    CLIENTS.lock().unwrap().iter().filter(|c| c.kind == kind).count()
}

pub fn clients_event() -> ServerEvent {
    ServerEvent::Clients { items: CLIENTS.lock().unwrap().clone() }
}

// ---------------------------------------------------------------- contexte et taches

pub fn context_event(config: &Config, current_place: &Mutex<Option<String>>) -> ServerEvent {
    ServerEvent::Context {
        place: current_place.lock().unwrap().clone(),
        quiet_start: config.quiet_hours.start.clone(),
        quiet_end: config.quiet_hours.end.clone(),
        quiet_active: config.quiet_hours.contains(chrono::Local::now().time()),
        auth_required: config.auth.is_some() || crate::passkey::has_any(),
        presence: crate::power::snapshot(),
        next_event: crate::calendar::next_event(),
        calendar_connected: crate::calendar::configured(&config.google),
        calendar_error: crate::calendar::last_error(),
        sleep: crate::sleep::snapshot(),
        memory_facts: crate::memory::global().and_then(|m| m.count().ok()),
    }
}

pub fn todos_event(todos: &TodoStore) -> ServerEvent {
    match todos.list(false) {
        Ok(items) => ServerEvent::Todos { items },
        Err(err) => {
            tracing::warn!(?err, "lecture des taches impossible pour le tableau de bord");
            ServerEvent::Todos { items: Vec::new() }
        }
    }
}

/// Echeance saisie dans l'UI (`2026-10-06T18:30`, secondes facultatives) -> epoch secondes.
pub fn parse_due_at(raw: &str) -> Option<i64> {
    use chrono::TimeZone;
    let raw = raw.trim();
    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M"))
        .ok()?;
    chrono::Local.from_local_datetime(&naive).single().map(|dt| dt.timestamp())
}

// ---------------------------------------------------------------- telemetrie

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
}

/// Dernieres latences mesurees (0 = pas encore mesure).
static STT_MS: AtomicU64 = AtomicU64::new(0);
static TTS_MS: AtomicU64 = AtomicU64::new(0);
static FIRST_WORD_MS: AtomicU64 = AtomicU64::new(0);
static GPU: Mutex<Option<GpuInfo>> = Mutex::new(None);

/// Duree de la derniere transcription Whisper.
#[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
pub fn record_stt_ms(ms: u64) {
    STT_MS.store(ms.max(1), Ordering::Relaxed);
}

/// Duree de la derniere synthese vocale (une phrase).
#[cfg_attr(not(feature = "full-audio"), allow(dead_code))]
pub fn record_tts_ms(ms: u64) {
    TTS_MS.store(ms.max(1), Ordering::Relaxed);
}

fn measured(value: &AtomicU64) -> Option<u64> {
    match value.load(Ordering::Relaxed) {
        0 => None,
        ms => Some(ms),
    }
}

pub fn telemetry_event() -> ServerEvent {
    let audio = cfg!(feature = "full-audio");
    ServerEvent::Telemetry {
        stt_model: audio.then(|| "Whisper large-v3-turbo".to_string()),
        tts_voice: audio
            .then(|| std::env::var("NESTORD_TTS_VOICE").unwrap_or_else(|_| "fr_FR-upmc-medium".to_string())),
        judge_model: crate::settings::get().judge_model,
        gpu: GPU.lock().unwrap().clone(),
        stt_ms: measured(&STT_MS),
        first_word_ms: measured(&FIRST_WORD_MS),
        tts_ms: measured(&TTS_MS),
    }
}

/// Interroge `nvidia-smi` (absent ou en erreur : pas de carte affichee).
async fn poll_gpu() -> Option<GpuInfo> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.used,memory.total", "--format=csv,noheader,nounits"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    parse_gpu_line(String::from_utf8_lossy(&output.stdout).lines().next()?)
}

fn parse_gpu_line(line: &str) -> Option<GpuInfo> {
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next()?.trim_start_matches("NVIDIA ").trim_start_matches("GeForce ").to_string();
    let memory_used_mb = parts.next()?.parse().ok()?;
    let memory_total_mb = parts.next()?.parse().ok()?;
    Some(GpuInfo { name, memory_used_mb, memory_total_mb })
}

/// Tache de fond du tableau de bord : mesure le delai avant le premier mot de la
/// reponse (en observant les evenements), sonde la carte graphique et rediffuse
/// periodiquement la telemetrie et le contexte (les heures calmes changent avec l'heure).
pub fn spawn_ticker(
    events_tx: broadcast::Sender<ServerEvent>,
    config: Arc<Config>,
    current_place: Arc<Mutex<Option<String>>>,
) {
    let mut events_rx = events_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut turn_started_ms: Option<u64> = None;
        let mut ticks: u64 = 0;
        loop {
            tokio::select! {
                event = events_rx.recv() => match event {
                    // Debut de tour : Nestor commence a reflechir.
                    Ok(ServerEvent::State { status: DaemonStatus::Thinking }) => {
                        turn_started_ms.get_or_insert_with(now_ms);
                    }
                    Ok(ServerEvent::Transcript { role: Role::Assistant, .. }) => {
                        if let Some(started) = turn_started_ms.take() {
                            FIRST_WORD_MS.store(now_ms().saturating_sub(started).max(1), Ordering::Relaxed);
                        }
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = interval.tick() => {
                    if ticks % 2 == 0 {
                        let gpu = poll_gpu().await;
                        *GPU.lock().unwrap() = gpu;
                    }
                    let _ = events_tx.send(telemetry_event());
                    if ticks % 6 == 0 {
                        let _ = events_tx.send(context_event(&config, &current_place));
                    }
                    ticks += 1;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classe_le_client_en_veille() {
        assert_eq!(classify_client(Some("standby"), Some("okhttp/4.12")), "standby");
        assert_eq!(classify_client(None, Some("okhttp/4.12")), "mobile");
    }

    #[test]
    fn type_de_client() {
        assert_eq!(classify_client(Some("mobile"), Some("Mozilla/5.0")), "mobile");
        assert_eq!(classify_client(None, Some("okhttp/4.12.0")), "mobile");
        assert_eq!(classify_client(None, Some("Mozilla/5.0 (X11; Linux x86_64) Chrome/154")), "web");
        assert_eq!(classify_client(None, Some("Python/3.14 websockets/15")), "autre");
        assert_eq!(classify_client(None, None), "autre");
    }

    #[test]
    fn ligne_nvidia_smi() {
        let gpu = parse_gpu_line("NVIDIA GeForce RTX 3060, 5123, 12288").unwrap();
        assert_eq!(gpu.name, "RTX 3060");
        assert_eq!((gpu.memory_used_mb, gpu.memory_total_mb), (5123, 12288));
        assert!(parse_gpu_line("erreur").is_none());
    }

    #[test]
    fn echeance_saisie_dans_l_ui() {
        assert!(parse_due_at("2026-10-06T18:30").is_some());
        assert_eq!(parse_due_at("2026-10-06T18:30:00"), parse_due_at("2026-10-06T18:30"));
        assert!(parse_due_at("demain").is_none());
    }

    #[test]
    fn inscription_et_retrait_d_un_client() {
        let id = register_client("web");
        assert!(CLIENTS.lock().unwrap().iter().any(|c| c.id == id));
        unregister_client(id);
        assert!(!CLIENTS.lock().unwrap().iter().any(|c| c.id == id));
    }
}
