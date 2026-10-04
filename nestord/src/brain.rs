//! Cerveau conversationnel de Nestor avec bascule automatique / mode reduit.
//!
//! En temps normal, la conversation principale est confiee a Claude Code CLI
//! (`claude -p`). Lorsque le quota Claude est epuise (rate limit 100%, code 429,
//! solde insuffisant) ou que le processus Claude s'arrete, Nestor bascule
//! automatiquement en « Mode Reduit » sur Antigravity (`agy -p`).
//!
//! En mode reduit :
//! - La personnalite de majordome et les contraintes vocales sont preservees.
//! - Le streaming `text_delta` est diffuse en direct a l'UI et au TTS Kokoro.
//! - La continuite de session est conservee via l'identifiant `--conversation`.
//! - Les appels d'outils (`step_type == "tool"`) sont publies vers la console.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::claude_process::ClaudeHandle;
use crate::config::Config;
use crate::protocol::{DaemonStatus, Role, ServerEvent, ToolCallStatus};
use crate::usage::UsageState;

const ENV_VARS_TO_SCRUB: &[&str] = &["ANTHROPIC_API_KEY", "CLAUDE_CODE_API_KEY", "CLAUDECODE"];

const SENTENCE_BOUNDARIES: &[char] = &['.', '!', '?', '\n'];
const MIN_TTS_SEGMENT_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveBackend {
    Claude,
    Agy,
}

impl ActiveBackend {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Agy => "agy",
        }
    }
}

pub struct NestorBrain {
    claude_handle: Arc<OnceLock<ClaudeHandle>>,
    events_tx: broadcast::Sender<ServerEvent>,
    tts_tx: Option<mpsc::UnboundedSender<String>>,
    usage: Arc<UsageState>,
    config: Arc<Config>,
    tokio_handle: tokio::runtime::Handle,
    active_backend: Arc<RwLock<ActiveBackend>>,
    is_fallback: Arc<AtomicBool>,
    fallback_reason: Arc<Mutex<Option<String>>>,
    agy_conversation_id: Arc<Mutex<Option<String>>>,
    is_processing: Arc<AtomicBool>,
    pending_user_message: Arc<Mutex<Option<String>>>,
    /// Message utilisateur en attente de confirmation orale suite a un
    /// verdict "Confirm" du juge de conscience (`judge.rs`).
    pending_judged_message: Arc<Mutex<Option<String>>>,
}

impl NestorBrain {
    pub fn new(
        claude_handle: Arc<OnceLock<ClaudeHandle>>,
        events_tx: broadcast::Sender<ServerEvent>,
        tts_tx: Option<mpsc::UnboundedSender<String>>,
        usage: Arc<UsageState>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            claude_handle,
            events_tx,
            tts_tx,
            usage,
            config,
            tokio_handle: tokio::runtime::Handle::current(),
            active_backend: Arc::new(RwLock::new(ActiveBackend::Claude)),
            is_fallback: Arc::new(AtomicBool::new(false)),
            fallback_reason: Arc::new(Mutex::new(None)),
            agy_conversation_id: Arc::new(Mutex::new(None)),
            is_processing: Arc::new(AtomicBool::new(false)),
            pending_user_message: Arc::new(Mutex::new(None)),
            pending_judged_message: Arc::new(Mutex::new(None)),
        }
    }

    #[allow(dead_code)]
    pub fn is_fallback(&self) -> bool {
        self.is_fallback.load(Ordering::Relaxed)
    }

    pub fn clear_pending_user_message(&self) {
        *self.pending_user_message.lock().unwrap() = None;
    }

    /// Etat actuel du moteur, pret a etre envoye a un nouveau client WebSocket.
    pub fn snapshot(&self) -> ServerEvent {
        let active = if self.is_fallback.load(Ordering::Relaxed) {
            "agy".to_string()
        } else {
            "claude".to_string()
        };
        let is_fallback = self.is_fallback.load(Ordering::Relaxed);
        let reason = self.fallback_reason.lock().unwrap().clone();

        ServerEvent::BackendStatus {
            active_backend: active,
            is_fallback,
            reason,
        }
    }

    /// Declenche la bascule vers le mode reduit AGY.
    pub async fn trigger_fallback(&self, reason: &str) {
        let already_fallback = self.is_fallback.swap(true, Ordering::SeqCst);
        *self.active_backend.write().await = ActiveBackend::Agy;
        *self.fallback_reason.lock().unwrap() = Some(reason.to_string());

        tracing::warn!(reason, "Bascule en Mode Reduit (Antigravity/AGY) activee");

        let _ = self.events_tx.send(ServerEvent::BackendStatus {
            active_backend: "agy".to_string(),
            is_fallback: true,
            reason: Some(reason.to_string()),
        });

        // Si ce n'etait pas deja en fallback, on annonce la bascule vocalement
        if !already_fallback {
            let announcement = "Monsieur, le quota de session Claude etant epuise, je bascule en mode reduit sur Antigravity pour assurer notre service.";
            let _ = self.events_tx.send(ServerEvent::Transcript {
                role: Role::Assistant,
                delta: None,
                text: announcement.to_string(),
                is_final: Some(true),
            });
            if let Some(ref tts_tx) = self.tts_tx {
                let _ = tts_tx.send(announcement.to_string());
            }
        }

        // Si une requete utilisateur etait en suspens (rejetee par Claude sans reponse),
        // on la rejoue immediatement sur AGY pour ne pas perdre la parole.
        let pending = self.pending_user_message.lock().unwrap().take();
        if let Some(prompt) = pending {
            tracing::info!(prompt = %prompt, "Rejeu automatique de la requete sur AGY suite a la bascule");
            let _ = self.run_agy_turn(&prompt).await;
        }
    }

    /// Permet de basculer explicitement de backend (claude, agy, ou auto).
    pub async fn set_backend(&self, target: &str) {
        match target.trim().to_ascii_lowercase().as_str() {
            "claude" => {
                self.is_fallback.store(false, Ordering::SeqCst);
                *self.active_backend.write().await = ActiveBackend::Claude;
                *self.fallback_reason.lock().unwrap() = None;
                self.usage.set_exhausted(false);
                let _ = self.events_tx.send(ServerEvent::BackendStatus {
                    active_backend: "claude".to_string(),
                    is_fallback: false,
                    reason: None,
                });
                tracing::info!("Backend conversationnel force sur Claude");
            }
            "agy" => {
                self.trigger_fallback("Mode reduit active manuellement").await;
            }
            "auto" => {
                if self.usage.is_exhausted() {
                    self.trigger_fallback("Quota Claude epuise (mode auto)").await;
                } else {
                    self.is_fallback.store(false, Ordering::SeqCst);
                    *self.active_backend.write().await = ActiveBackend::Claude;
                    *self.fallback_reason.lock().unwrap() = None;
                    let _ = self.events_tx.send(ServerEvent::BackendStatus {
                        active_backend: "claude".to_string(),
                        is_fallback: false,
                        reason: None,
                    });
                    tracing::info!("Backend conversationnel remis en mode automatique (Claude)");
                }
            }
            _ => tracing::warn!(target, "backend inconnu ignore"),
        }
    }

    /// Envoie un message utilisateur a l'assistant, apres evaluation par le
    /// juge de conscience local (`judge.rs`). Un rapport interne (compte
    /// rendu de mission, rappel de tache) n'a pas a passer par le juge :
    /// utiliser [`Self::send_internal_report`] pour ceux-la.
    pub async fn send_user_message(&self, content: &str) -> Result<()> {
        let pending = self.pending_judged_message.lock().unwrap().take();
        if let Some(pending) = pending {
            if is_affirmative(content) {
                return self.dispatch(&pending).await;
            }
            // Reponse ambigue ou negative : on abandonne la demande en
            // suspens plutot que de deviner, et on traite le message comme
            // une nouvelle demande a part entiere.
        }

        let judgement = crate::judge::evaluate(&self.config.judge, content, content).await;
        match judgement.decision {
            crate::judge::Decision::Allow => self.dispatch(content).await,
            crate::judge::Decision::Confirm => {
                let rationale = judgement.verdict.map(|v| v.rationale).unwrap_or_default();
                *self.pending_judged_message.lock().unwrap() = Some(content.to_string());
                let announcement = format!(
                    "Un instant, {} - cette demande me semble a risque : {rationale} Confirmez-vous ?",
                    self.config.address_form
                );
                self.announce(&announcement).await;
                Ok(())
            }
            crate::judge::Decision::Deny => {
                let rationale = judgement.verdict.map(|v| v.rationale).unwrap_or_default();
                let announcement =
                    format!("Je ne donnerai pas suite, {} : {rationale}", self.config.address_form);
                self.announce(&announcement).await;
                Ok(())
            }
        }
    }

    /// Envoie un rapport interne (compte rendu de mission, rappel de tache)
    /// directement a l'assistant, sans passer par le juge de conscience : ce
    /// n'est pas une demande de l'utilisateur mais un evenement deja survenu
    /// ou une relance generee par nestord lui-meme.
    pub async fn send_internal_report(&self, report: &str) -> Result<()> {
        self.dispatch(report).await
    }

    /// Annonce un message directement (transcript + TTS), sans passer par un
    /// tour de conversation : utilise pour les verdicts du juge, qui n'ont
    /// pas besoin d'etre reformules par le modele.
    async fn announce(&self, text: &str) {
        let _ = self.events_tx.send(ServerEvent::Transcript {
            role: Role::Assistant,
            delta: None,
            text: text.to_string(),
            is_final: Some(true),
        });
        if let Some(ref tts_tx) = self.tts_tx {
            let _ = tts_tx.send(text.to_string());
        }
    }

    /// Route effectivement le message vers Claude ou AGY selon le backend
    /// actif, avec bascule automatique en cas d'echec. Ne juge rien : c'est
    /// le role des appelants ([`Self::send_user_message`]).
    async fn dispatch(&self, content: &str) -> Result<()> {
        let backend = *self.active_backend.read().await;
        let is_exhausted = self.usage.is_exhausted();

        if backend == ActiveBackend::Claude && !is_exhausted {
            *self.pending_user_message.lock().unwrap() = Some(content.to_string());
            if let Some(claude) = self.claude_handle.get() {
                match claude.send_user_message(content).await {
                    Ok(_) => return Ok(()),
                    Err(err) => {
                        tracing::warn!(?err, "Echec de communication avec Claude, repli sur AGY");
                        self.trigger_fallback("Sous-processus Claude injoignable").await;
                        return Ok(());
                    }
                }
            } else {
                tracing::warn!("Session Claude non prete, repli sur AGY");
                self.trigger_fallback("Session Claude non prete").await;
                return Ok(());
            }
        }

        // Execution en Mode Reduit avec Antigravity (AGY)
        *self.pending_user_message.lock().unwrap() = None;
        self.run_agy_turn(content).await
    }

    /// Variante appelable depuis la boucle audio sans bloquer le thread.
    #[allow(dead_code)]
    pub fn send_user_message_from_audio(self: &Arc<Self>, content: &str) {
        let brain = self.clone();
        let text = content.to_string();
        self.tokio_handle.spawn(async move {
            if let Err(err) = brain.send_user_message(&text).await {
                tracing::error!(?err, "Erreur lors du traitement de la requete vocale");
            }
        });
    }

    /// Execute un tour de parole en mode reduit avec le CLI `agy`.
    async fn run_agy_turn(&self, user_text: &str) -> Result<()> {
        if self.is_processing.swap(true, Ordering::SeqCst) {
            tracing::warn!("Un tour agy est deja en cours, rejet de la requete concurrente");
            return Ok(());
        }

        let _ = self.events_tx.send(ServerEvent::State {
            status: DaemonStatus::Thinking,
        });

        let mut cmd = Command::new("agy");
        cmd.args(["--output-format", "stream-json", "--dangerously-skip-permissions"]);

        for var in ENV_VARS_TO_SCRUB {
            cmd.env_remove(var);
        }

        let conv_id = self.agy_conversation_id.lock().unwrap().clone();
        if let Some(ref cid) = conv_id {
            cmd.arg(format!("--conversation={cid}"));
        }

        let address = &self.config.address_form;

        // Conditionnement majordome en francais parle
        let prompt = if conv_id.is_none() {
            format!(
                "Tu es Nestor, majordome a l'ancienne au service de {address}. \
                Tu es actuellement en mode reduit de secours propulse par Antigravity. \
                Contraintes vocales strictes : reponds en 1 a 3 phrases maximum, \
                en francais parle et naturel, sans aucun markdown, bloc de code, liste, titre ni emoji. \
                Va droit au but. Demande de {address} : {user_text}"
            )
        } else {
            user_text.to_string()
        };

        cmd.arg(format!("-p={prompt}"));

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context("impossible de lancer agy en mode reduit")?;

        let stdout = child.stdout.take().context("stdout agy indisponible")?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(target: "agy::stderr", "{line}");
                }
            });
        }

        let mut lines = BufReader::new(stdout).lines();
        let mut turn_state = TurnState::default();

        let events_tx = self.events_tx.clone();
        let tts_tx = self.tts_tx.clone();
        let agy_conv_id = self.agy_conversation_id.clone();
        let is_processing = self.is_processing.clone();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };

                let event_name = value.get("event").and_then(Value::as_str).unwrap_or_default();
                match event_name {
                    "init" => {
                        if let Some(cid) = value.get("conversation_id").and_then(Value::as_str) {
                            let mut lock = agy_conv_id.lock().unwrap();
                            if lock.is_none() {
                                *lock = Some(cid.to_string());
                                tracing::info!(conversation_id = cid, "Session AGY initialisee");
                            }
                        }
                    }
                    "step_update" => {
                        let Some(step) = value.get("step_update") else { continue };
                        let step_type = step.get("step_type").and_then(Value::as_str).unwrap_or_default();
                        match step_type {
                            "agent_response" => {
                                if let Some(delta) = step.get("text_delta").and_then(Value::as_str) {
                                    let sentences = turn_state.push_delta(delta);
                                    let _ = events_tx.send(ServerEvent::Transcript {
                                        role: Role::Assistant,
                                        delta: Some(delta.to_string()),
                                        text: turn_state.full_text.clone(),
                                        is_final: Some(false),
                                    });
                                    for sentence in sentences {
                                        if let Some(ref tts) = tts_tx {
                                            let _ = tts.send(sentence);
                                        }
                                    }
                                }
                            }
                            "tool" => {
                                let name = step.get("tool_name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                let input = step.pointer("/tool_info/parameters").cloned().unwrap_or(Value::Null);
                                let done = step.get("state").and_then(Value::as_str) == Some("DONE");

                                let _ = events_tx.send(ServerEvent::ToolCall {
                                    name,
                                    input,
                                    status: if done { ToolCallStatus::Completed } else { ToolCallStatus::Running },
                                    mission_id: None,
                                });
                            }
                            _ => {}
                        }
                    }
                    "result" => {
                        // Flush du dernier segment vers le TTS
                        if let (Some(tts), Some(leftover)) = (tts_tx.as_ref(), sanitize_for_tts(&turn_state.sentence_buf)) {
                            let _ = tts.send(leftover);
                        }

                        let _ = events_tx.send(ServerEvent::Transcript {
                            role: Role::Assistant,
                            delta: None,
                            text: turn_state.full_text.clone(),
                            is_final: Some(true),
                        });
                        let _ = events_tx.send(ServerEvent::State {
                            status: DaemonStatus::Idle,
                        });
                    }
                    _ => {}
                }
            }

            let _ = child.wait().await;
            is_processing.store(false, Ordering::SeqCst);
        });

        Ok(())
    }
}

#[derive(Default)]
struct TurnState {
    full_text: String,
    sentence_buf: String,
}

impl TurnState {
    fn push_delta(&mut self, delta: &str) -> Vec<String> {
        self.full_text.push_str(delta);
        self.sentence_buf.push_str(delta);

        let mut segments = Vec::new();
        loop {
            let Some(idx) = self.sentence_buf.find(SENTENCE_BOUNDARIES) else {
                break;
            };
            let boundary_len = self.sentence_buf[idx..].chars().next().unwrap().len_utf8();
            let boundary_end = idx + boundary_len;

            if self.sentence_buf[..boundary_end].trim().chars().count() < MIN_TTS_SEGMENT_CHARS
                && self.sentence_buf.len() > boundary_end
            {
                break;
            }

            let segment: String = self.sentence_buf.drain(..boundary_end).collect();
            if let Some(spoken) = sanitize_for_tts(&segment) {
                segments.push(spoken);
            }
        }
        segments
    }
}

/// Detecte une confirmation orale a une question de type "confirmez-vous ?".
/// Volontairement permissif plutot que de faire attendre {address} un mot
/// magique precis ; toute reponse ambigue est traitee comme un refus.
fn is_affirmative(text: &str) -> bool {
    let normalized = text.trim().to_ascii_lowercase();
    const PHRASES: &[&str] =
        &["oui", "vas-y", "vas y", "confirme", "confirmé", "je confirme", "fais-le", "fais le", "d'accord", "ok", "okay"];
    PHRASES.iter().any(|p| normalized == *p || normalized.starts_with(&format!("{p} ")))
}

fn sanitize_for_tts(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '`' | '*' | '_' | '#' | '|' | '>' | '[' | ']'))
        .collect();

    let cleaned = cleaned.trim().trim_start_matches('-').trim();
    if cleaned.is_empty() || !cleaned.chars().any(char::is_alphanumeric) {
        return None;
    }
    Some(cleaned.to_string())
}
