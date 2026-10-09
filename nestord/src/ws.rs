//! Serveur WebSocket Axum expose a l'UI web et a l'appli mobile sur `/ws`.
//!
//! - Texte (JSON) : `ServerEvent`/`ClientEvent`, cf. `protocol.rs`. L'audio
//!   TTS de sortie transite aussi ici, en JSON (`AudioChunk`, PCM16 base64) -
//!   voir la note dans `audio/mod.rs` sur pourquoi ce n'est pas une frame
//!   binaire brute.
//! - Binaire (PCM16LE mono 16 kHz) : audio micro, client -> serveur uniquement.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::protocol::{ClientEvent, DaemonStatus, ServerEvent};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct AppState {
    /// Secret exige sur `/mcp`, remis au seul assistant (cf. `auth.rs`).
    pub mcp_secret: String,
    pub events_tx: tokio::sync::broadcast::Sender<ServerEvent>,
    /// Configuration utilisateur (forme d'adresse, lieux, heures calmes, reveil).
    pub config: Arc<Config>,
    /// Dernier lieu reconnu (`Config::place_at`) a partir de la position GPS
    /// recue du front. `None` si aucune position recue ou lieu inconnu.
    pub current_place: Arc<Mutex<Option<String>>>,
    /// Memoire de taches (ponctuelles et recurrentes), cf. `todo.rs`.
    pub todos: Arc<crate::todo::TodoStore>,
    /// Compteur incremente a chaque barge-in : le pipeline TTS (feature
    /// `full-audio`) l'observe pour abandonner les segments d'un tour interrompu.
    pub barge_in_gen: Arc<AtomicU64>,
    /// Valeur de `barge_in_gen` au debut du dernier tour utilisateur : tant que
    /// `barge_in_gen` la depasse, la reponse interrompue n'est plus synthetisee.
    pub turn_started_gen: Arc<AtomicU64>,
    /// Audio micro recu du front (frames binaires), transmis au pipeline VAD/STT.
    pub mic_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Horodatage (epoch ms) jusqu'auquel la synthese est censee etre jouee :
    /// le pipeline d'ecoute ignore le micro pendant cette fenetre (usage micro
    /// + haut-parleurs, donc Nestor se reentend). Remis a zero sur barge-in.
    pub speaking_until_ms: Arc<AtomicU64>,
    /// Horodatage (epoch ms) jusqu'auquel le dialogue est actif sans exiger le mot-cle.
    pub wake_active_until_ms: Arc<AtomicU64>,
    /// Missions deleguees a des sous-agents (outil MCP `start_mission`).
    pub missions: Arc<crate::mission::MissionManager>,
    /// Consommation du quota de la session, pour l'instantane de connexion.
    pub usage: Arc<crate::usage::UsageState>,
    /// Moteur conversationnel unifie (Claude Code avec repli automatique sur AGY).
    pub brain: Arc<crate::brain::NestorBrain>,
}

fn connection_snapshot(state: &AppState) -> Vec<ServerEvent> {
    let mut events = Vec::new();

    events.push(state.brain.snapshot());
    events.push(ServerEvent::Settings { settings: crate::settings::get() });
    events.extend(state.brain.judge_snapshot());
    events.push(crate::dashboard::context_event(&state.config, &state.current_place));
    events.push(crate::dashboard::todos_event(&state.todos));
    events.push(crate::dashboard::clients_event());
    events.push(crate::dashboard::telemetry_event());
    events.push(crate::connectors::event());
    events.extend(crate::proactive::recent_alerts());
    if let Some(connectors) = crate::connectors::global() {
        events.extend(connectors.pending_events());
    }

    let is_wake_active = now_ms() < state.wake_active_until_ms.load(Ordering::SeqCst);
    events.push(ServerEvent::WakeState { active: is_wake_active });
    events.push(ServerEvent::State {
        status: if is_wake_active || !crate::settings::get().wake_word_enabled {
            DaemonStatus::Listening
        } else {
            DaemonStatus::Idle
        },
    });

    if let Some((five_hour, seven_day, resets_at)) = state.usage.snapshot() {
        events.push(ServerEvent::Usage { five_hour, seven_day, resets_at });
    }

    for mission in state.missions.list() {
        events.push(ServerEvent::Mission {
            id: mission.id,
            backend: mission.backend,
            status: mission.status,
            description: mission.description,
            summary: mission.summary,
            progress: mission.progress,
        });
    }

    events
}

#[derive(serde::Deserialize)]
pub struct WsAuthQuery {
    token: Option<String>,
    /// Type de client annonce (`web`, `mobile`), pour le panneau « Appareils ».
    client: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(auth): Query<WsAuthQuery>,
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let origin = headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok());
    if !crate::auth::origin_allowed(origin, &state.config.allowed_origins) {
        tracing::warn!(?origin, "connexion /ws refusee : origine web non autorisee");
        return (StatusCode::FORBIDDEN, "origine non autorisee").into_response();
    }
    // Acces protege des qu'un jeton est configure ou qu'une passkey est enregistree. Deux
    // preuves acceptees : le jeton (application mobile) ou une session ouverte par passkey.
    if state.config.auth.is_some() || crate::passkey::has_any() {
        let presented = auth.token.as_deref().unwrap_or_default();
        let by_token = state.config.auth.as_ref().is_some_and(|expected| expected.matches(presented));
        if !by_token && !crate::passkey::session_valid(presented) {
            tracing::warn!("connexion /ws refusee : jeton ou session invalide");
            return (StatusCode::UNAUTHORIZED, "authentification requise").into_response();
        }
    }

    let user_agent = headers.get(axum::http::header::USER_AGENT).and_then(|v| v.to_str().ok());
    let kind = crate::dashboard::classify_client(auth.client.as_deref(), user_agent);
    ws.on_upgrade(move |socket| handle_socket(socket, state, kind)).into_response()
}

/// Retire le client de la liste des appareils a la fin de la connexion, quelle qu'en soit la cause.
struct ClientGuard {
    id: u64,
    events_tx: tokio::sync::broadcast::Sender<ServerEvent>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        crate::dashboard::unregister_client(self.id);
        let _ = self.events_tx.send(crate::dashboard::clients_event());
    }
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, kind: &'static str) {
    let (mut sender, mut receiver) = socket.split();
    let mut events_rx = state.events_tx.subscribe();

    // Inscrit avant l'instantane, pour que ce client se voie lui-meme dans la liste.
    let _client = ClientGuard { id: crate::dashboard::register_client(kind), events_tx: state.events_tx.clone() };
    let _ = state.events_tx.send(crate::dashboard::clients_event());

    // Instantane de connexion : les evenements sont diffuses en direct et ne
    // sont pas rejoues, donc un client qui arrive (ou se reconnecte) en cours
    // de route ne verrait ni les missions ni le quota sans ceci.
    for event in connection_snapshot(&state) {
        let Ok(json) = serde_json::to_string(&event) else { continue };
        if sender.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    // Rappel de debut de conversation : une nouvelle connexion (appel,
    // ouverture de l'UI) est le meilleur proxy dont on dispose pour « le
    // debut d'une conversation ». Le cooldown de TodoStore::due_now evite le
    // spam si plusieurs clients se connectent en peu de temps.
    let todos = state.todos.clone();
    let brain_for_nudge = state.brain.clone();
    let address_form = state.config.address_form.clone();
    tokio::spawn(async move {
        let due = match todos.due_now() {
            Ok(due) => due,
            Err(err) => {
                tracing::error!(?err, "echec de lecture des taches dues (nudge de connexion)");
                return;
            }
        };
        if due.is_empty() {
            return;
        }
        let ids: Vec<i64> = due.iter().map(|t| t.id).collect();
        let report = crate::proactive::todo_nudge_report(&due, &address_form);
        if brain_for_nudge.send_internal_report(&report).await.is_ok() {
            let _ = todos.mark_notified(&ids);
        }
    });

    // Tache sortante : relaie tous les ServerEvent (JSON, y compris AudioChunk) vers ce client.
    let mut outgoing = tokio::spawn(async move {
        while let Ok(event) = events_rx.recv().await {
            let Ok(json) = serde_json::to_string(&event) else { continue };
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    let events_tx = state.events_tx.clone();
    let barge_in_gen = state.barge_in_gen.clone();
    let turn_started_gen = state.turn_started_gen.clone();
    let todos_store = state.todos.clone();
    let mic_tx = state.mic_tx.clone();
    let speaking_until_ms = state.speaking_until_ms.clone();
    let wake_active_until_ms = state.wake_active_until_ms.clone();
    let missions = state.missions.clone();
    let brain = state.brain.clone();
    let current_place = state.current_place.clone();
    let config = state.config.clone();

    // Tache entrante : traite les ClientEvent (JSON) et l'audio micro (binaire).
    let mut incoming = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Binary(bytes) => {
                    // Best-effort : en cas de saturation on prefere perdre une
                    // frame audio plutot que bloquer la lecture du socket.
                    if let Err(err) = mic_tx.try_send(bytes.to_vec()) {
                        tracing::warn!(?err, len = bytes.len(), "frame audio micro non transmise (channel plein/ferme)");
                    } else {
                        tracing::trace!(len = bytes.len(), "frame audio micro recue");
                    }
                }
                Message::Text(text) => match serde_json::from_str::<ClientEvent>(&text) {
                    Ok(ClientEvent::BargeIn) => {
                        tracing::info!("barge-in recu, interruption de la synthese en cours");
                        barge_in_gen.fetch_add(1, Ordering::SeqCst);
                        // Le front a stoppe sa lecture : le micro redevient
                        // exploitable immediatement.
                        speaking_until_ms.store(0, Ordering::SeqCst);
                        let timeout_ms = crate::settings::get().wake_timeout_secs.max(3) * 1000;
                        wake_active_until_ms.store(now_ms() + timeout_ms, Ordering::SeqCst);
                        let _ = events_tx.send(ServerEvent::WakeState { active: true });
                        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
                    }
                    Ok(ClientEvent::TodoAdd { title, due_at, recurrence }) => {
                        let title = title.trim();
                        let recurrence = recurrence.as_deref().and_then(crate::todo::Recurrence::parse);
                        let due_at = due_at.as_deref().and_then(crate::dashboard::parse_due_at);
                        if title.is_empty() {
                            tracing::debug!("tache sans intitule ignoree");
                        } else if let Err(err) = todos_store.add(title, None, due_at, recurrence) {
                            tracing::error!(?err, "echec d'ajout de tache depuis l'UI");
                        }
                        let _ = events_tx.send(crate::dashboard::todos_event(&todos_store));
                    }
                    Ok(ClientEvent::TodoComplete { id }) => {
                        if let Err(err) = todos_store.complete(id) {
                            tracing::error!(?err, id, "echec de mise a jour de tache depuis l'UI");
                        }
                        let _ = events_tx.send(crate::dashboard::todos_event(&todos_store));
                    }
                    Ok(ClientEvent::TodoDelete { id }) => {
                        if let Err(err) = todos_store.delete(id) {
                            tracing::error!(?err, id, "echec de suppression de tache depuis l'UI");
                        }
                        let _ = events_tx.send(crate::dashboard::todos_event(&todos_store));
                    }
                    Ok(ClientEvent::SetToolMode { server, tool, mode }) => {
                        if let Some(connectors) = crate::connectors::global() {
                            connectors.set_tool_mode(&server, &tool, mode);
                        }
                    }
                    Ok(ClientEvent::ResolveToolApproval { id, approve }) => {
                        tracing::info!(id, approve, "ecriture externe tranchee depuis l'UI");
                        if let Some(connectors) = crate::connectors::global() {
                            connectors.resolve_approval(id, approve);
                        }
                    }
                    Ok(ClientEvent::ResolveJudgement { id, approve }) => {
                        tracing::info!(id, approve, "confirmation du juge tranchee depuis l'UI");
                        turn_started_gen.store(barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
                        if let Err(err) = brain.resolve_judgement(id, approve).await {
                            tracing::error!(?err, "echec du traitement de la confirmation");
                        }
                    }
                    Ok(ClientEvent::UpdateSettings { settings }) => {
                        let applied = crate::settings::update(settings);
                        tracing::info!(?applied, "reglages modifies depuis l'UI");
                        let _ = events_tx.send(ServerEvent::Settings { settings: applied });
                    }
                    Ok(ClientEvent::SendText { content }) => {
                        let timeout_ms = crate::settings::get().wake_timeout_secs.max(3) * 1000;
                        wake_active_until_ms.store(now_ms() + timeout_ms, Ordering::SeqCst);
                        let _ = events_tx.send(ServerEvent::WakeState { active: true });
                        let _ = events_tx.send(ServerEvent::Transcript {
                            role: crate::protocol::Role::User,
                            delta: None,
                            text: content.clone(),
                            is_final: Some(true),
                        });
                        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
                        turn_started_gen.store(barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
                        if let Err(err) = brain.send_user_message(&content).await {
                            tracing::error!(?err, "echec d'envoi du message utilisateur");
                        }
                    }
                    Ok(ClientEvent::SetBackend { backend }) => {
                        tracing::info!(backend, "changement de backend demande par le client");
                        brain.set_backend(&backend).await;
                    }
                    Ok(ClientEvent::StopMission { id, reason }) => {
                        let reason = reason
                            .filter(|r| !r.trim().is_empty())
                            .unwrap_or_else(|| "arret demande depuis l'interface".to_string());
                        if missions.cancel(id, Some(reason)) {
                            tracing::info!(id, "annulation de mission demandee depuis l'UI");
                        } else {
                            tracing::debug!(id, "mission inconnue ou deja terminee, annulation ignoree");
                        }
                    }
                    Ok(ClientEvent::AudioIn { .. }) => {
                        // Doublon volontaire du front (meme audio que la frame binaire
                        // deja recue) : ignore pour eviter un double traitement.
                    }
                    Ok(ClientEvent::Location { lat, lon }) => {
                        let place = config.place_at(lat, lon).map(str::to_string);
                        let mut current = current_place.lock().unwrap();
                        if *current != place {
                            tracing::info!(?place, "changement de lieu detecte");
                            *current = place;
                            drop(current);
                            let _ = events_tx.send(crate::dashboard::context_event(&config, &current_place));
                        }
                    }
                    Err(err) => {
                        tracing::debug!(?err, raw = %text, "message client non reconnu");
                    }
                },
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut outgoing => incoming.abort(),
        _ = &mut incoming => outgoing.abort(),
    }
}
