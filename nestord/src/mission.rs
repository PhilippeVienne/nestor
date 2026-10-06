//! Delegation de missions a des sous-agents.
//!
//! La session conversationnelle doit rester reactive : elle ne fait donc pas
//! elle-meme le travail long. Elle appelle l'outil MCP `start_mission`, qui
//! lance ici un processus dedie (`claude -p` ou `agy -p`), suit son flux, en
//! publie les appels d'outils vers l'UI, puis reinjecte un compte rendu dans
//! la conversation pour que Nestor l'annonce a voix haute.

use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot};

use crate::claude_process::ClaudeHandle;
use crate::protocol::{MissionStatus, ServerEvent, ToolCallStatus};
use crate::usage::UsageState;

/// Au-dela de ce remplissage de la fenetre de quota Claude, les missions sans
/// backend explicite partent sur `agy` : une mission longue qui se fait couper
/// en plein milieu par la limite est pire qu'une mission confiee a l'autre agent.
const USAGE_ROUTING_THRESHOLD: f32 = 0.85;

/// Variables d'environnement a purger, comme pour la session conversationnelle.
const ENV_VARS_TO_SCRUB: &[&str] = &["ANTHROPIC_API_KEY", "CLAUDE_CODE_API_KEY", "CLAUDECODE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Claude,
    Agy,
}

impl Backend {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "agy" | "antigravity" => Some(Self::Agy),
            _ => None,
        }
    }

    fn binary(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Agy => "agy",
        }
    }

    fn label(self) -> &'static str {
        self.binary()
    }
}

#[derive(Debug, Clone)]
pub struct MissionRecord {
    pub id: u64,
    pub backend: String,
    pub description: String,
    pub status: MissionStatus,
    pub summary: Option<String>,
    /// Derniere activite connue du sous-agent, pour repondre a « ou en est
    /// la mission ? » sans attendre le compte rendu final.
    pub progress: Option<String>,
}

pub struct MissionManager {
    next_id: AtomicU64,
    missions: Mutex<Vec<MissionRecord>>,
    /// Signal d'annulation par mission en cours : consomme a l'annulation,
    /// retire a la fin de la mission.
    cancels: Mutex<HashMap<u64, oneshot::Sender<String>>>,
    events_tx: broadcast::Sender<ServerEvent>,
    claude: Arc<OnceLock<ClaudeHandle>>,
    usage: Arc<UsageState>,
}

impl MissionManager {
    pub fn new(
        events_tx: broadcast::Sender<ServerEvent>,
        claude: Arc<OnceLock<ClaudeHandle>>,
        usage: Arc<UsageState>,
    ) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            missions: Mutex::new(Vec::new()),
            cancels: Mutex::new(HashMap::new()),
            events_tx,
            claude,
            usage,
        }
    }

    pub fn list(&self) -> Vec<MissionRecord> {
        self.missions.lock().expect("verrou missions empoisonne").clone()
    }

    /// Choisit le backend : celui demande explicitement, sinon Claude tant que
    /// son quota le permet.
    fn route(&self, requested: Option<Backend>) -> (Backend, Option<String>) {
        if let Some(backend) = requested {
            return (backend, None);
        }
        match self.usage.worst_utilization() {
            Some(utilization) if utilization >= USAGE_ROUTING_THRESHOLD => (
                Backend::Agy,
                Some(format!("quota Claude a {:.0} %, mission routee vers agy", utilization * 100.0)),
            ),
            _ => (Backend::Claude, None),
        }
    }

    /// Enregistre et demarre une mission. Retourne immediatement : le travail
    /// se poursuit dans une tache de fond.
    pub fn start(self: &Arc<Self>, description: String, requested: Option<Backend>) -> MissionRecord {
        let (backend, routing_note) = self.route(requested);
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let record = MissionRecord {
            id,
            backend: backend.label().to_string(),
            description: description.clone(),
            status: MissionStatus::Started,
            summary: None,
            progress: None,
        };

        self.missions.lock().expect("verrou missions empoisonne").push(record.clone());
        let _ = self.events_tx.send(ServerEvent::Mission {
            id,
            backend: record.backend.clone(),
            status: MissionStatus::Started,
            description: description.clone(),
            summary: routing_note,
            progress: None,
        });

        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancels.lock().expect("verrou annulations empoisonne").insert(id, cancel_tx);

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = manager.run(id, backend, &description, cancel_rx).await;
            manager.cancels.lock().expect("verrou annulations empoisonne").remove(&id);
            manager.finish(id, backend, &description, outcome);
        });

        record
    }

    fn finish(&self, id: u64, backend: Backend, description: &str, outcome: Result<MissionOutcome>) {
        let (status, summary) = match outcome {
            Ok(MissionOutcome::Done(summary)) => (MissionStatus::Completed, summary),
            Ok(MissionOutcome::Cancelled { reason, partial }) => {
                tracing::info!(id, %reason, "mission annulee");
                let summary = match partial {
                    Some(partial) => format!(
                        "annulee avant la fin ({reason}). Travail deja produit : {partial}"
                    ),
                    None => format!(
                        "annulee avant la fin ({reason}). Aucun resultat partiel n'avait ete produit."
                    ),
                };
                (MissionStatus::Cancelled, summary)
            }
            Err(err) => {
                tracing::error!(?err, id, "mission en echec");
                (MissionStatus::Failed, format!("echec : {err}"))
            }
        };

        if let Ok(mut missions) = self.missions.lock() {
            if let Some(entry) = missions.iter_mut().find(|m| m.id == id) {
                entry.status = status;
                entry.summary = Some(summary.clone());
            }
        }

        let _ = self.events_tx.send(ServerEvent::Mission {
            id,
            backend: backend.label().to_string(),
            status,
            description: description.to_string(),
            summary: Some(summary.clone()),
            progress: None,
        });

        // Reinjecte le compte rendu dans la conversation : c'est Nestor qui
        // l'annonce, avec ses contraintes de concision, plutot qu'un texte brut
        // pousse directement au TTS.
        let claude = self.claude.get().cloned();
        let verdict = match status {
            MissionStatus::Completed => "terminee",
            MissionStatus::Cancelled => "annulee",
            _ => "en echec",
        };
        let report = format!(
            "[Rapport interne : mission {id} ({}) {verdict}. Objet : {description}. Resultat : {summary}] \
Annonce ce resultat a l'utilisateur en une ou deux phrases.",
            backend.label()
        );
        tokio::spawn(async move {
            let Some(claude) = claude else {
                tracing::error!("session conversationnelle indisponible, rapport de mission perdu");
                return;
            };
            if let Err(err) = claude.send_user_message(&report).await {
                tracing::error!(?err, "impossible de transmettre le rapport de mission a la conversation");
            }
        });
    }

    /// Demande l'arret d'une mission en cours, avec un motif qui sera repris
    /// dans le compte rendu. Retourne `false` si la mission est inconnue ou
    /// deja terminee.
    pub fn cancel(&self, id: u64, reason: Option<String>) -> bool {
        let sender = self.cancels.lock().expect("verrou annulations empoisonne").remove(&id);
        match sender {
            Some(sender) => sender.send(reason.unwrap_or_else(|| "sans motif precise".to_string())).is_ok(),
            None => false,
        }
    }

    /// Enregistre et diffuse l'avancement d'une mission en cours.
    fn report_progress(&self, id: u64, activity: String) {
        let mut snapshot = None;
        if let Ok(mut missions) = self.missions.lock() {
            if let Some(entry) = missions.iter_mut().find(|m| m.id == id) {
                entry.progress = Some(activity.clone());
                snapshot = Some((entry.backend.clone(), entry.description.clone()));
            }
        }
        let Some((backend, description)) = snapshot else { return };

        let _ = self.events_tx.send(ServerEvent::Mission {
            id,
            backend,
            status: MissionStatus::Started,
            description,
            summary: None,
            progress: Some(activity),
        });
    }

    /// Lance le sous-agent et suit son flux jusqu'au resultat final.
    async fn run(
        &self,
        id: u64,
        backend: Backend,
        description: &str,
        cancel_rx: oneshot::Receiver<String>,
    ) -> Result<MissionOutcome> {
        let mut cmd = Command::new(backend.binary());
        cmd.args(["--output-format", "stream-json"]);
        match backend {
            Backend::Claude => {
                // Mode auto du CLI : un classifieur autorise ou refuse chaque action (shell,
                // reseau...) a la place d'un humain, au lieu de tout laisser passer. Un refus
                // n'arrete pas la mission : le sous-agent recoit le motif et continue. Ce
                // n'est pas une garantie de surete (cf. `claude_process.rs`).
                cmd.args(["--permission-mode", "auto"]);
                // `-p` est un booleen et le prompt un argument positionnel ;
                // `--verbose` est impose avec --output-format stream-json.
                // Un sous-agent de mission n'a acces a aucun serveur MCP ni connecteur du compte :
                // il travaille sur le code, pas sur la messagerie de l'utilisateur.
                cmd.args(["--verbose", "--strict-mcp-config", "-p", description]);
            }
            Backend::Agy => {
                // Parsing de flags a la Go : le prompt doit etre attache au
                // flag, sinon `-p` avale l'option suivante comme prompt.
                // `agy` n'a pas de mode auto : il garde l'autorisation generique.
                cmd.arg("--dangerously-skip-permissions");
                cmd.arg(format!("-p={description}"));
            }
        }

        for var in ENV_VARS_TO_SCRUB {
            cmd.env_remove(var);
        }
        // Secrets que l'assistant n'a pas a connaitre : jeton de nestord, identifiants des connecteurs.
        for var in crate::auth::secret_env_vars() {
            cmd.env_remove(var);
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("impossible de lancer le backend '{}'", backend.binary()))?;

        let stdout = child.stdout.take().context("stdout du sous-agent indisponible")?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(target: "mission::stderr", "{line}");
                }
            });
        }

        let mut lines = BufReader::new(stdout).lines();
        let mut outcome = StreamOutcome::default();
        let mut cancel_rx = cancel_rx;
        let mut cancel_reason: Option<String> = None;

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                    match backend {
                        Backend::Claude => self.parse_claude_line(id, &value, &mut outcome),
                        Backend::Agy => self.parse_agy_line(id, &value, &mut outcome),
                    }
                }
                reason = &mut cancel_rx => {
                    // Annulation demandee : on tue le sous-agent sans attendre
                    // qu'il finisse son tour en cours.
                    let _ = child.kill().await;
                    cancel_reason = Some(reason.unwrap_or_else(|_| "annulation demandee".to_string()));
                    break;
                }
            }
        }

        // Le flux est coupe : les outils encore ouverts n'auront jamais leur
        // retour d'execution, il faut les refermer explicitement pour l'UI.
        self.close_open_tools(id, &mut outcome);

        if let Some(reason) = cancel_reason {
            let partial = [outcome.final_text.trim(), outcome.last_text.trim()]
                .into_iter()
                .find(|text| !text.is_empty())
                .map(truncate_partial);
            return Ok(MissionOutcome::Cancelled { reason, partial });
        }

        let status = child.wait().await.context("attente de la fin du sous-agent")?;
        anyhow::ensure!(status.success(), "le sous-agent s'est termine avec {status}");
        if let Some(failure) = outcome.failure {
            anyhow::bail!("{failure}");
        }

        let summary = if !outcome.final_text.trim().is_empty() {
            outcome.final_text
        } else if !outcome.last_text.trim().is_empty() {
            outcome.last_text
        } else {
            "aucun compte rendu produit".to_string()
        };

        Ok(MissionOutcome::Done(summary.trim().to_string()))
    }

    /// Schema du CLI Claude : enveloppe `type`, contenu facon Messages API.
    fn parse_claude_line(&self, id: u64, value: &Value, outcome: &mut StreamOutcome) {
        match value.get("type").and_then(Value::as_str).unwrap_or_default() {
            "assistant" => {
                let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
                    return;
                };
                for block in content {
                    match block.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            let name =
                                block.get("name").and_then(Value::as_str).unwrap_or("unknown").to_string();
                            let input = block.get("input").cloned().unwrap_or(Value::Null);
                            let tool_id =
                                block.get("id").and_then(Value::as_str).unwrap_or_default().to_string();

                            self.emit_tool(id, &name, input.clone(), ToolCallStatus::Running);
                            self.report_progress(id, describe_activity(&name, &input));
                            outcome.open_tools.insert(tool_id, (name, input));
                        }
                        Some("text") => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                outcome.last_text = text.to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Retour d'execution d'outil : c'est Claude Code qui l'emet, pas
            // l'utilisateur. Sans ce cas, aucun outil n'etait jamais clos.
            "user" => {
                let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
                    return;
                };
                for block in content {
                    if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let Some(tool_id) = block.get("tool_use_id").and_then(Value::as_str) else { continue };
                    if let Some((name, input)) = outcome.open_tools.remove(tool_id) {
                        self.emit_tool(id, &name, input, ToolCallStatus::Completed);
                    }
                }
            }
            "result" => {
                if let Some(text) = value.get("result").and_then(Value::as_str) {
                    outcome.final_text = text.to_string();
                }
            }
            _ => {}
        }
    }

    /// Schema du CLI Antigravity : enveloppe `event`, etapes `step_update`.
    /// Format entierement different de celui de Claude, d'ou ce second parseur.
    fn parse_agy_line(&self, id: u64, value: &Value, outcome: &mut StreamOutcome) {
        match value.get("event").and_then(Value::as_str).unwrap_or_default() {
            "step_update" => {
                let Some(step) = value.get("step_update") else { return };
                match step.get("step_type").and_then(Value::as_str).unwrap_or_default() {
                    "tool" => {
                        let name =
                            step.get("tool_name").and_then(Value::as_str).unwrap_or("unknown").to_string();
                        let input = step.pointer("/tool_info/parameters").cloned().unwrap_or(Value::Null);
                        let done = step.get("state").and_then(Value::as_str) == Some("DONE");

                        // agy n'expose pas d'identifiant d'appel : on suit par
                        // nom, suffisant pour refermer ce qui reste ouvert.
                        if done {
                            outcome.open_tools.remove(&name);
                            self.emit_tool(id, &name, input, ToolCallStatus::Completed);
                        } else {
                            outcome.open_tools.insert(name.clone(), (name.clone(), input.clone()));
                            self.report_progress(id, describe_activity(&name, &input));
                            self.emit_tool(id, &name, input, ToolCallStatus::Running);
                        }
                    }
                    "agent_response" => {
                        if let Some(delta) = step.get("text_delta").and_then(Value::as_str) {
                            outcome.last_text.push_str(delta);
                        }
                    }
                    _ => {}
                }
            }
            "result" => {
                let Some(result) = value.get("result") else { return };
                if let Some(text) = result.get("response").and_then(Value::as_str) {
                    outcome.final_text = text.to_string();
                }
                match result.get("status").and_then(Value::as_str) {
                    Some("SUCCESS") | None => {}
                    Some(status) => outcome.failure = Some(format!("agy a termine avec le statut {status}")),
                }
            }
            _ => {}
        }
    }

    /// Referme les appels d'outils restes ouverts (mission annulee, process
    /// tue, ou flux tronque). On les clot en `completed` plutot qu'avec un
    /// statut dedie : c'est le statut de la *mission* qui porte l'information
    /// d'annulation ou d'echec, et les autres clients (mobile) n'ont pas a
    /// connaitre un nouveau statut d'outil.
    fn close_open_tools(&self, id: u64, outcome: &mut StreamOutcome) {
        if outcome.open_tools.is_empty() {
            return;
        }
        tracing::debug!(
            id,
            count = outcome.open_tools.len(),
            "cloture des appels d'outils restes ouverts"
        );
        for (_, (name, input)) in outcome.open_tools.drain() {
            self.emit_tool(id, &name, input, ToolCallStatus::Completed);
        }
    }

    fn emit_tool(&self, id: u64, name: &str, input: Value, status: ToolCallStatus) {
        let _ = self.events_tx.send(ServerEvent::ToolCall {
            name: name.to_string(),
            input,
            status,
            mission_id: Some(id),
        });
    }
}

/// Issue d'une mission menee a son terme ou interrompue.
enum MissionOutcome {
    Done(String),
    /// Motif de l'annulation et travail deja produit, pour ne pas perdre le
    /// resultat partiel d'une mission arretee en cours de route.
    Cancelled { reason: String, partial: Option<String> },
}

/// Resume d'un appel d'outil en une ligne lisible, pour le suivi d'avancement.
fn describe_activity(name: &str, input: &Value) -> String {
    // On privilegie les champs qui disent *sur quoi* l'outil travaille.
    let detail = ["file_path", "path", "command", "CommandLine", "pattern", "query"]
        .into_iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .map(|value| value.chars().take(80).collect::<String>());

    match detail {
        Some(detail) => format!("{name} : {detail}"),
        None => name.to_string(),
    }
}

/// Tronque un resultat partiel : il part dans un compte rendu lu a voix haute.
fn truncate_partial(text: &str) -> String {
    const LIMIT: usize = 600;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let head: String = text.chars().take(LIMIT).collect();
    format!("{head} [...]")
}

/// Ce qu'on retient du flux d'un sous-agent, quel que soit son format.
#[derive(Default)]
struct StreamOutcome {
    /// Compte rendu final annonce par le backend.
    final_text: String,
    /// Dernier texte vu, utilise si le backend ne fournit pas de resultat final.
    last_text: String,
    failure: Option<String>,
    /// Outils annonces mais pas encore clos, par identifiant (ou par nom pour
    /// les backends qui n'en fournissent pas). Sert a fermer ce qui reste
    /// ouvert quand le flux s'arrete : sinon l'UI les affiche « en cours »
    /// indefiniment.
    open_tools: HashMap<String, (String, Value)>,
}
