//! Serveur WebSocket Axum expose a l'UI Antigravity sur `/ws`.
//!
//! - Texte (JSON) : `ServerEvent`/`ClientEvent`, cf. `protocol.rs`. L'audio
//!   TTS de sortie transite aussi ici, en JSON (`AudioChunk`, PCM16 base64) -
//!   voir la note dans `audio/mod.rs` sur pourquoi ce n'est pas une frame
//!   binaire brute.
//! - Binaire (PCM16LE mono 16 kHz) : audio micro, client -> serveur uniquement.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::protocol::{ClientEvent, DaemonStatus, ServerEvent};

#[derive(Clone)]
pub struct AppState {
    pub events_tx: tokio::sync::broadcast::Sender<ServerEvent>,
    /// Compteur incremente a chaque barge-in : le pipeline TTS (feature
    /// `full-audio`) l'observe pour abandonner les segments d'un tour interrompu.
    pub barge_in_gen: Arc<AtomicU64>,
    /// Audio micro recu du front (frames binaires), transmis au pipeline VAD/STT.
    pub mic_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Horodatage (epoch ms) jusqu'auquel la synthese est censee etre jouee :
    /// le pipeline d'ecoute ignore le micro pendant cette fenetre (usage micro
    /// + haut-parleurs, donc Nestor se reentend). Remis a zero sur barge-in.
    pub speaking_until_ms: Arc<AtomicU64>,
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

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut events_rx = state.events_tx.subscribe();

    // Instantane de connexion : les evenements sont diffuses en direct et ne
    // sont pas rejoues, donc un client qui arrive (ou se reconnecte) en cours
    // de route ne verrait ni les missions ni le quota sans ceci.
    for event in connection_snapshot(&state) {
        let Ok(json) = serde_json::to_string(&event) else { continue };
        if sender.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

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
    let mic_tx = state.mic_tx.clone();
    let speaking_until_ms = state.speaking_until_ms.clone();
    let missions = state.missions.clone();
    let brain = state.brain.clone();

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
                        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
                    }
                    Ok(ClientEvent::SendText { content }) => {
                        let _ = events_tx.send(ServerEvent::Transcript {
                            role: crate::protocol::Role::User,
                            delta: None,
                            text: content.clone(),
                            is_final: Some(true),
                        });
                        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
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
