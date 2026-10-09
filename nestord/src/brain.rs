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
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pending_judged_message: Arc<Mutex<Option<(u64, String)>>>,
    /// Dernieres decisions du juge, pour le panneau « Conscience » de l'UI.
    judge_log: Arc<Mutex<VecDeque<JudgeEntry>>>,
    judge_seq: Arc<AtomicU64>,
    /// Compteur de barge-in et sa valeur au debut du dernier tour (cf. `ws::AppState`).
    barge_in_gen: Arc<AtomicU64>,
    turn_started_gen: Arc<AtomicU64>,
}

/// Nombre de decisions du juge conservees pour l'instantane de connexion.
const JUDGE_LOG_LEN: usize = 30;

#[derive(Debug, Clone)]
struct JudgeEntry {
    id: u64,
    source: &'static str,
    text: String,
    decision: &'static str,
    score: Option<u8>,
    category: Option<String>,
    rationale: Option<String>,
    at_ms: u64,
}

impl JudgeEntry {
    fn event(&self, pending: bool) -> ServerEvent {
        ServerEvent::JudgeVerdict {
            id: self.id,
            source: self.source.to_string(),
            text: self.text.clone(),
            decision: self.decision.to_string(),
            score: self.score,
            category: self.category.clone(),
            rationale: self.rationale.clone(),
            pending,
            at_ms: self.at_ms,
        }
    }
}

impl NestorBrain {
    pub fn new(
        claude_handle: Arc<OnceLock<ClaudeHandle>>,
        events_tx: broadcast::Sender<ServerEvent>,
        tts_tx: Option<mpsc::UnboundedSender<String>>,
        usage: Arc<UsageState>,
        config: Arc<Config>,
        barge_in_gen: Arc<AtomicU64>,
        turn_started_gen: Arc<AtomicU64>,
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
            judge_log: Arc::new(Mutex::new(VecDeque::new())),
            judge_seq: Arc::new(AtomicU64::new(0)),
            barge_in_gen,
            turn_started_gen,
        }
    }

    /// Enregistre et diffuse une decision du juge. `pending` : une confirmation
    /// de l'utilisateur est attendue. Retourne l'identifiant de la decision.
    pub fn record_verdict(
        &self,
        source: &'static str,
        text: &str,
        judgement: &crate::judge::Judgement,
        pending: bool,
    ) -> u64 {
        let id = self.judge_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let entry = JudgeEntry {
            id,
            source,
            text: text.to_string(),
            decision: match judgement.decision {
                crate::judge::Decision::Allow => "allow",
                crate::judge::Decision::Confirm => "confirm",
                crate::judge::Decision::Deny => "deny",
            },
            score: judgement.verdict.as_ref().map(|v| v.risk_score),
            category: judgement.verdict.as_ref().map(|v| v.category.clone()),
            rationale: judgement.verdict.as_ref().map(|v| v.rationale.clone()),
            at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };
        let _ = self.events_tx.send(entry.event(pending));
        let mut log = self.judge_log.lock().unwrap();
        log.push_back(entry);
        while log.len() > JUDGE_LOG_LEN {
            log.pop_front();
        }
        id
    }

    /// Decisions recentes du juge, pour l'instantane envoye a un client qui se connecte.
    pub fn judge_snapshot(&self) -> Vec<ServerEvent> {
        let pending_id = self.pending_judged_message.lock().unwrap().as_ref().map(|(id, _)| *id);
        self.judge_log.lock().unwrap().iter().map(|e| e.event(pending_id == Some(e.id))).collect()
    }

    /// Tranche, depuis l'UI, une confirmation demandee par le juge. Sans effet si
    /// `id` ne correspond plus a la demande en attente (deja tranchee a la voix).
    pub async fn resolve_judgement(&self, id: u64, approve: bool) -> Result<()> {
        let pending = {
            let mut guard = self.pending_judged_message.lock().unwrap();
            match guard.as_ref() {
                Some((pending_id, _)) if *pending_id == id => guard.take(),
                _ => None,
            }
        };
        let Some((_, content)) = pending else {
            tracing::info!(id, "confirmation deja tranchee ou inconnue, ignoree");
            return Ok(());
        };
        let _ = self.events_tx.send(ServerEvent::JudgeResolved { id, approved: approve });
        if approve {
            let _ = self.events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
            self.dispatch(&content).await
        } else {
            self.announce(&format!("Tres bien, {}, j'abandonne cette demande.", self.config.address_form)).await;
            Ok(())
        }
    }

    pub fn is_fallback(&self) -> bool {
        self.is_fallback.load(Ordering::Relaxed)
    }

    /// Motif de la derniere bascule en mode reduit.
    pub fn fallback_reason(&self) -> Option<String> {
        self.fallback_reason.lock().unwrap().clone()
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
        // Une ecriture externe attend un accord : la reponse de l'utilisateur la tranche,
        // elle n'est pas transmise a l'assistant comme une nouvelle demande.
        if let Some(connectors) = crate::connectors::global() {
            match (connectors.pending_count(), classify_answer(content)) {
                (0, _) => {}
                // Une seule demande et une reponse nette : on tranche.
                (1, Some(approve)) => {
                    connectors.resolve_oldest(approve);
                    return Ok(());
                }
                // Reponse ambigue : ni accord ni refus, la demande reste en attente.
                (1, None) => {
                    self.announce_to_user(
                        "je n'ai pas compris. Dites oui pour confirmer cette action, ou non pour l'annuler.",
                    )
                    .await;
                    return Ok(());
                }
                // Plusieurs demandes : un « oui » ne dirait pas laquelle.
                _ => {
                    self.announce_to_user("plusieurs actions attendent votre accord : repondez a l'ecran pour chacune.")
                        .await;
                    return Ok(());
                }
            }
        }

        let pending = self.pending_judged_message.lock().unwrap().take();
        if let Some((id, pending)) = pending {
            let approved = is_affirmative(content);
            let _ = self.events_tx.send(ServerEvent::JudgeResolved { id, approved });
            if approved {
                return self.dispatch(&pending).await;
            }
            // Reponse ambigue ou negative : on abandonne la demande en
            // suspens plutot que de deviner, et on traite le message comme
            // une nouvelle demande a part entiere.
        }

        let judge_config = crate::settings::get().judge_config(&self.config.judge);
        let judgement = crate::judge::evaluate(&judge_config, content, content).await;
        match judgement.decision {
            crate::judge::Decision::Allow => {
                self.record_verdict("message", content, &judgement, false);
                self.dispatch(content).await
            }
            crate::judge::Decision::Confirm => {
                let id = self.record_verdict("message", content, &judgement, true);
                let rationale = judgement.verdict.map(|v| v.rationale).unwrap_or_default();
                *self.pending_judged_message.lock().unwrap() = Some((id, content.to_string()));
                let announcement = format!(
                    "Un instant, {} - cette demande me semble a risque : {rationale} Confirmez-vous ?",
                    self.config.address_form
                );
                self.announce(&announcement).await;
                Ok(())
            }
            crate::judge::Decision::Deny => {
                self.record_verdict("message", content, &judgement, false);
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
    /// Previent l'utilisateur a la voix (confirmation d'une ecriture externe).
    pub async fn announce_to_user(&self, text: &str) {
        self.announce(&format!("{}, {text}", self.config.address_form)).await;
    }

    /// Ouvre un nouveau tour de parole : ce qui va etre dit n'appartient pas a une reponse
    /// interrompue. Sans cela, apres une interruption non suivie d'un tour utilisateur
    /// (bouton stop, parole ecartee), comptes rendus et annonces resteraient muets.
    fn mark_turn_start(&self) {
        self.turn_started_gen.store(self.barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
    }

    async fn announce(&self, text: &str) {
        self.mark_turn_start();
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
        self.mark_turn_start();
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
        // Secrets que l'assistant n'a pas a connaitre : jeton de nestord, identifiants des connecteurs.
        for var in crate::auth::secret_env_vars() {
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

/// Reponse a une demande de confirmation : `Some(true)` pour un accord, `Some(false)`
/// pour un refus, `None` pour tout le reste.
///
/// Seules des reponses breves et sans ambiguite comptent : « ok lance plutot les tests »
/// n'est pas un accord. La ponctuation et la casse ajoutees par la transcription
/// (« Oui. ») sont ignorees.
fn classify_answer(text: &str) -> Option<bool> {
    let normalized: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'à' | 'â' => 'a',
            'ç' => 'c',
            '\u{2019}' => '\'',
            c if c.is_alphanumeric() || c == '\'' => c,
            _ => ' ',
        })
        .collect();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    const YES: &[&str] = &[
        "oui", "oui oui", "oui vas y", "vas y", "oui je confirme", "je confirme", "confirme", "c'est confirme",
        "d'accord", "oui d'accord", "ok", "okay", "ok vas y", "fais le", "oui fais le", "c'est bon", "approuve",
        "j'approuve", "oui merci", "oui s'il te plait", "oui s'il vous plait",
    ];
    const NO: &[&str] = &[
        "non", "non non", "non merci", "annule", "annuler", "non annule", "refuse", "je refuse", "stop",
        "surtout pas", "non surtout pas", "ne fais pas ca", "non ne fais pas ca", "laisse tomber",
    ];
    if YES.contains(&normalized.as_str()) {
        Some(true)
    } else if NO.contains(&normalized.as_str()) {
        Some(false)
    } else {
        None
    }
}

fn is_affirmative(text: &str) -> bool {
    classify_answer(text) == Some(true)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accord_et_refus_nets() {
        // La transcription ajoute casse et ponctuation.
        for yes in ["Oui.", "oui", "Oui, vas-y.", "Je confirme.", "D'accord !", "OK", "C’est bon."] {
            assert_eq!(classify_answer(yes), Some(true), "{yes}");
        }
        for no in ["Non.", "Non merci.", "Annule.", "Surtout pas !", "Laisse tomber"] {
            assert_eq!(classify_answer(no), Some(false), "{no}");
        }
    }

    #[test]
    fn reponse_ambigue_n_est_ni_accord_ni_refus() {
        for unclear in [
            "Ok lance plutot les tests.",
            "Oui mais attends.",
            "C'est quoi cette action ?",
            "Confirmez-vous ?",
            "Non, envoie-le plutot a Marie.",
            "",
        ] {
            assert_eq!(classify_answer(unclear), None, "{unclear}");
        }
        assert!(!is_affirmative("ok lance plutot les tests"));
    }
}
