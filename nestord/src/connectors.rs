//! Connecteurs : serveurs MCP externes (messagerie, agenda...) relayes par nestord.
//!
//! L'assistant tourne avec `--dangerously-skip-permissions` : lui donner
//! directement un serveur MCP personnel reviendrait a le laisser ecrire dans la
//! messagerie ou l'agenda sans controle. nestord fait donc **passerelle** :
//! l'assistant ne voit que le serveur MCP de nestord, qui relaie les outils
//! externes sous le nom `<serveur>__<outil>` et applique, dans le code :
//!
//! - **liste blanche** : un outil en mode `off` n'est ni liste ni appelable ;
//! - **lecture libre** (`read`) : relaye tel quel ;
//! - **confirmation** (`confirm`) : l'appel reste bloque jusqu'a l'accord de
//!   l'utilisateur (bouton de l'UI ou « oui » a la voix), refuse apres un delai.
//!
//! Par defaut, un outil annonce en lecture seule par son serveur
//! (`annotations.readOnlyHint`) est en lecture libre ; tout autre outil exige une
//! confirmation. Les serveurs se declarent dans `config.toml` (`[[mcp_servers]]`,
//! secrets par variables d'environnement) ; les modes choisis dans l'UI sont
//! enregistres dans `~/.config/nestord/ui-connectors.toml`.
//!
//! Aucun connecteur externe n'est demarre sans jeton d'authentification
//! (`auth_token`) : `/ws` et `/mcp` donneraient sinon ces acces a tout processus local.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, oneshot};

use crate::protocol::ServerEvent;

/// Delai de reponse d'un serveur externe.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(60);
/// Delai accorde a l'utilisateur pour confirmer une ecriture.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const PROTOCOL_VERSION: &str = "2025-06-18";

// ---------------------------------------------------------------- configuration

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolMode {
    /// Lecture : relaye sans demander.
    Read,
    /// Ecriture : confirmation de l'utilisateur avant chaque appel.
    Confirm,
    /// Non expose a l'assistant.
    Off,
}

/// Un serveur MCP externe, declare dans `config.toml` :
///
/// ```toml
/// [[mcp_servers]]
/// name = "agenda"
/// url = "https://exemple.net/mcp"            # ou : command = "npx", args = ["-y", "serveur-mcp"]
/// headers = { Authorization = "Bearer ${AGENDA_TOKEN}" }
/// tools = { creer_evenement = "confirm", supprimer_evenement = "off" }
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct McpServerConfig {
    pub name: String,
    /// Serveur HTTP (« streamable HTTP »).
    pub url: Option<String>,
    /// Serveur local lance par nestord (transport stdio).
    pub command: Option<String>,
    pub args: Vec<String>,
    /// Variables d'environnement du processus ; `${VAR}` est remplace par l'environnement de nestord.
    pub env: HashMap<String, String>,
    /// En-tetes HTTP ; `${VAR}` est remplace de meme (pour ne pas ecrire de secret dans le fichier).
    pub headers: HashMap<String, String>,
    /// Mode impose par outil, prioritaire sur le mode par defaut.
    pub tools: HashMap<String, ToolMode>,
}

/// Remplace les `${VAR}` par les variables d'environnement (vide si absente).
fn expand_env(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find('}') {
            Some(end) => {
                out.push_str(&std::env::var(&rest[start + 2..start + 2 + end]).unwrap_or_default());
                rest = &rest[start + 2 + end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Nom d'outil accepte par MCP : `[A-Za-z0-9_-]`, 64 caracteres au plus.
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn exposed_name(server: &str, tool: &str) -> String {
    let mut name = format!("{}__{}", sanitize(server), sanitize(tool));
    name.truncate(64);
    name
}

// ---------------------------------------------------------------- transport

type PendingReplies = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

enum Transport {
    Http {
        client: reqwest::Client,
        url: String,
        headers: Vec<(String, String)>,
        session: Mutex<Option<String>>,
    },
    Stdio {
        stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
        pending: PendingReplies,
        /// Garde le processus en vie ; il est tue a l'abandon (`kill_on_drop`).
        _child: tokio::process::Child,
    },
}

struct Upstream {
    transport: Transport,
    next_id: AtomicU64,
}

impl Upstream {
    async fn connect(config: &McpServerConfig) -> Result<Self> {
        let transport = if let Some(url) = &config.url {
            Transport::Http {
                client: reqwest::Client::builder().timeout(UPSTREAM_TIMEOUT).build()?,
                url: url.clone(),
                headers: config.headers.iter().map(|(k, v)| (k.clone(), expand_env(v))).collect(),
                session: Mutex::new(None),
            }
        } else if let Some(command) = &config.command {
            let mut child = tokio::process::Command::new(command)
                .args(&config.args)
                .envs(config.env.iter().map(|(k, v)| (k.clone(), expand_env(v))))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("lancement de `{command}`"))?;
            let stdin = child.stdin.take().context("stdin du serveur MCP")?;
            let stdout = child.stdout.take().context("stdout du serveur MCP")?;
            let pending: PendingReplies = Arc::new(Mutex::new(HashMap::new()));
            let replies = pending.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    let Some(id) = message.get("id").and_then(Value::as_u64) else { continue };
                    if let Some(tx) = replies.lock().unwrap().remove(&id) {
                        let _ = tx.send(message);
                    }
                }
                // Processus termine : les appels en attente echouent (emetteurs abandonnes).
                replies.lock().unwrap().clear();
            });
            Transport::Stdio { stdin: tokio::sync::Mutex::new(stdin), pending, _child: child }
        } else {
            anyhow::bail!("ni `url` ni `command` : serveur MCP mal declare");
        };

        let upstream = Self { transport, next_id: AtomicU64::new(1) };
        upstream
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "nestord", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await
            .context("initialisation MCP")?;
        upstream.notify("notifications/initialized").await;
        Ok(upstream)
    }

    /// Envoie une requete JSON-RPC et retourne son `result`.
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let reply = match &self.transport {
            Transport::Http { client, url, headers, session } => {
                let mut request = client
                    .post(url)
                    .header("Accept", "application/json, text/event-stream")
                    .header("MCP-Protocol-Version", PROTOCOL_VERSION)
                    .json(&body);
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                let session_id = session.lock().unwrap().clone();
                if let Some(session_id) = session_id {
                    request = request.header("Mcp-Session-Id", session_id);
                }
                let response = request.send().await?.error_for_status()?;
                if let Some(new_session) = response.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
                    *session.lock().unwrap() = Some(new_session.to_string());
                }
                let is_sse = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|ct| ct.contains("text/event-stream"));
                let text = response.text().await?;
                if is_sse { parse_sse_reply(&text, id)? } else { serde_json::from_str(&text)? }
            }
            Transport::Stdio { stdin, pending, .. } => {
                let (tx, rx) = oneshot::channel();
                pending.lock().unwrap().insert(id, tx);
                let mut line = serde_json::to_vec(&body)?;
                line.push(b'\n');
                {
                    let mut stdin = stdin.lock().await;
                    stdin.write_all(&line).await?;
                    stdin.flush().await?;
                }
                match tokio::time::timeout(UPSTREAM_TIMEOUT, rx).await {
                    Ok(Ok(reply)) => reply,
                    Ok(Err(_)) => anyhow::bail!("le serveur MCP s'est arrete"),
                    Err(_) => {
                        pending.lock().unwrap().remove(&id);
                        anyhow::bail!("delai depasse");
                    }
                }
            }
        };
        if let Some(error) = reply.get("error") {
            anyhow::bail!("{}", error.get("message").and_then(Value::as_str).unwrap_or("erreur du serveur MCP"));
        }
        reply.get("result").cloned().context("reponse MCP sans `result`")
    }

    async fn notify(&self, method: &str) {
        let body = json!({ "jsonrpc": "2.0", "method": method });
        match &self.transport {
            Transport::Http { client, url, headers, session } => {
                let mut request = client.post(url).header("Accept", "application/json, text/event-stream").json(&body);
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                let session_id = session.lock().unwrap().clone();
                if let Some(session_id) = session_id {
                    request = request.header("Mcp-Session-Id", session_id);
                }
                let _ = request.send().await;
            }
            Transport::Stdio { stdin, .. } => {
                if let Ok(mut line) = serde_json::to_vec(&body) {
                    line.push(b'\n');
                    let mut stdin = stdin.lock().await;
                    let _ = stdin.write_all(&line).await;
                    let _ = stdin.flush().await;
                }
            }
        }
    }
}

/// Extrait d'un flux SSE la reponse JSON-RPC portant l'identifiant attendu.
fn parse_sse_reply(text: &str, id: u64) -> Result<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .find(|message| message.get("id").and_then(Value::as_u64) == Some(id))
        .context("reponse absente du flux SSE")
}

// ---------------------------------------------------------------- etat

#[derive(Debug, Clone)]
struct RemoteTool {
    name: String,
    description: String,
    input_schema: Value,
    read_only: bool,
}

enum Status {
    Connecting,
    Connected,
    Error(String),
    /// Non demarre (regle de securite), avec la raison.
    Disabled(String),
}

struct Server {
    config: McpServerConfig,
    status: RwLock<Status>,
    upstream: RwLock<Option<Arc<Upstream>>>,
    tools: RwLock<Vec<RemoteTool>>,
}

struct PendingApproval {
    server: String,
    tool: String,
    arguments: String,
    at_ms: u64,
    tx: oneshot::Sender<bool>,
}

pub struct Connectors {
    servers: Vec<Arc<Server>>,
    /// Modes choisis dans l'UI : (serveur, outil) -> mode.
    overrides: RwLock<HashMap<String, HashMap<String, ToolMode>>>,
    overrides_path: PathBuf,
    approvals: Mutex<HashMap<u64, PendingApproval>>,
    approval_seq: AtomicU64,
    events_tx: broadcast::Sender<ServerEvent>,
}

/// Mode par defaut d'un outil : la lecture seule annoncee par le serveur est libre,
/// tout le reste exige une confirmation.
fn default_mode(tool: &RemoteTool) -> ToolMode {
    if tool.read_only { ToolMode::Read } else { ToolMode::Confirm }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

static GLOBAL: OnceLock<Arc<Connectors>> = OnceLock::new();

/// Les connecteurs du daemon, une fois `init` appele.
pub fn global() -> Option<&'static Arc<Connectors>> {
    GLOBAL.get()
}

/// Declare les serveurs de la configuration et lance leur connexion en arriere-plan.
pub fn init(configs: &[McpServerConfig], auth_configured: bool, events_tx: broadcast::Sender<ServerEvent>) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let overrides_path = PathBuf::from(home).join(".config/nestord/ui-connectors.toml");
    let connectors = Connectors::build(configs, auth_configured, events_tx, overrides_path);

    if !connectors.servers.is_empty() && !auth_configured {
        tracing::warn!(
            "connecteurs MCP externes declares mais non demarres : aucun auth_token configure \
(`nestord onboard`). Sans jeton, tout processus local pourrait utiliser ces acces."
        );
    }
    if auth_configured {
        for server in connectors.servers.clone() {
            let connectors = connectors.clone();
            tokio::spawn(async move { connectors.connect(&server).await });
        }
    }
    let _ = GLOBAL.set(connectors);
}

impl Connectors {
    fn build(
        configs: &[McpServerConfig],
        auth_configured: bool,
        events_tx: broadcast::Sender<ServerEvent>,
        overrides_path: PathBuf,
    ) -> Arc<Self> {
        let overrides = std::fs::read_to_string(&overrides_path)
            .ok()
            .and_then(|raw| toml::from_str::<HashMap<String, HashMap<String, ToolMode>>>(&raw).ok())
            .unwrap_or_default();

        let servers = configs
            .iter()
            .filter(|c| !c.name.trim().is_empty())
            .map(|config| {
                let status = if auth_configured {
                    Status::Connecting
                } else {
                    Status::Disabled("jeton d'acces requis : lancez `nestord onboard` puis redemarrez".to_string())
                };
                Arc::new(Server {
                    config: config.clone(),
                    status: RwLock::new(status),
                    upstream: RwLock::new(None),
                    tools: RwLock::new(Vec::new()),
                })
            })
            .collect();

        Arc::new(Self {
            servers,
            overrides: RwLock::new(overrides),
            overrides_path,
            approvals: Mutex::new(HashMap::new()),
            approval_seq: AtomicU64::new(0),
            events_tx,
        })
    }

    /// Connecte un serveur et lit la liste de ses outils.
    async fn connect(&self, server: &Server) {
        let outcome = async {
            let upstream = Upstream::connect(&server.config).await?;
            let listing = upstream.request("tools/list", json!({})).await?;
            anyhow::Ok((upstream, parse_tools(&listing)))
        }
        .await;
        match outcome {
            Ok((upstream, tools)) => {
                tracing::info!(server = %server.config.name, tools = tools.len(), "connecteur MCP externe connecte");
                *server.tools.write().unwrap() = tools;
                *server.upstream.write().unwrap() = Some(Arc::new(upstream));
                *server.status.write().unwrap() = Status::Connected;
            }
            Err(err) => {
                tracing::warn!(server = %server.config.name, ?err, "connecteur MCP externe en erreur");
                *server.status.write().unwrap() = Status::Error(format!("{err:#}"));
            }
        }
        let _ = self.events_tx.send(self.event());
    }
}

fn parse_tools(listing: &Value) -> Vec<RemoteTool> {
    listing
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| {
                    Some(RemoteTool {
                        name: tool.get("name")?.as_str()?.to_string(),
                        description: tool.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                        input_schema: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                        read_only: tool.pointer("/annotations/readOnlyHint").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- vue pour l'UI

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Mode applique (absent pour les outils internes de nestord).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ToolMode>,
    /// Mode qu'aurait l'outil sans reglage (annonce de lecture seule ou non).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<ToolMode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    pub name: String,
    /// "interne" (serveur MCP de nestord) ou "externe".
    pub kind: String,
    /// "connected", "connecting", "error" ou "disabled".
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub tools: Vec<ToolInfo>,
}

fn internal_connector() -> ConnectorInfo {
    ConnectorInfo {
        name: "nestor".to_string(),
        kind: "interne".to_string(),
        status: "connected".to_string(),
        detail: None,
        tools: crate::mcp::tool_names()
            .into_iter()
            .map(|name| ToolInfo { name, description: String::new(), mode: None, default_mode: None })
            .collect(),
    }
}

/// Evenement `connectors` sans connecteur externe (avant `init`, tests).
pub fn event() -> ServerEvent {
    match global() {
        Some(connectors) => connectors.event(),
        None => ServerEvent::Connectors { items: vec![internal_connector()] },
    }
}

impl Connectors {
    fn mode_of(&self, server: &Server, tool: &RemoteTool) -> ToolMode {
        if let Some(mode) = self.overrides.read().unwrap().get(&server.config.name).and_then(|t| t.get(&tool.name)) {
            return *mode;
        }
        server.config.tools.get(&tool.name).copied().unwrap_or_else(|| default_mode(tool))
    }

    pub fn event(&self) -> ServerEvent {
        let mut items = vec![internal_connector()];
        for server in &self.servers {
            let (status, detail) = match &*server.status.read().unwrap() {
                Status::Connecting => ("connecting", None),
                Status::Connected => ("connected", None),
                Status::Error(err) => ("error", Some(err.clone())),
                Status::Disabled(reason) => ("disabled", Some(reason.clone())),
            };
            let tools = server
                .tools
                .read()
                .unwrap()
                .iter()
                .map(|tool| ToolInfo {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    mode: Some(self.mode_of(server, tool)),
                    default_mode: Some(default_mode(tool)),
                })
                .collect();
            items.push(ConnectorInfo {
                name: server.config.name.clone(),
                kind: "externe".to_string(),
                status: status.to_string(),
                detail,
                tools,
            });
        }
        ServerEvent::Connectors { items }
    }

    /// Outils externes exposes a l'assistant (ni `off`, ni serveur deconnecte).
    pub fn tool_definitions(&self) -> Vec<Value> {
        let mut definitions = Vec::new();
        for server in &self.servers {
            if !matches!(*server.status.read().unwrap(), Status::Connected) {
                continue;
            }
            for tool in server.tools.read().unwrap().iter() {
                let mode = self.mode_of(server, tool);
                if mode == ToolMode::Off {
                    continue;
                }
                let note = if mode == ToolMode::Confirm {
                    " (Ecriture : l'utilisateur doit confirmer, l'appel attend sa reponse.)"
                } else {
                    ""
                };
                definitions.push(json!({
                    "name": exposed_name(&server.config.name, &tool.name),
                    "description": format!("[{}] {}{note}", server.config.name, tool.description),
                    "inputSchema": tool.input_schema,
                }));
            }
        }
        definitions
    }

    fn find(&self, exposed: &str) -> Option<(Arc<Server>, RemoteTool)> {
        self.servers.iter().find_map(|server| {
            let tool = server
                .tools
                .read()
                .unwrap()
                .iter()
                .find(|tool| exposed_name(&server.config.name, &tool.name) == exposed)
                .cloned()?;
            Some((server.clone(), tool))
        })
    }

    /// Change le mode d'un outil depuis l'UI et l'enregistre.
    pub fn set_tool_mode(&self, server: &str, tool: &str, mode: ToolMode) {
        let known = self.servers.iter().any(|s| s.config.name == server && s.tools.read().unwrap().iter().any(|t| t.name == tool));
        if !known {
            tracing::debug!(server, tool, "outil inconnu, mode ignore");
            return;
        }
        let snapshot = {
            let mut overrides = self.overrides.write().unwrap();
            overrides.entry(server.to_string()).or_default().insert(tool.to_string(), mode);
            overrides.clone()
        };
        let saved = toml::to_string_pretty(&snapshot).map_err(anyhow::Error::from).and_then(|text| {
            if let Some(dir) = self.overrides_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&self.overrides_path, text)?;
            Ok(())
        });
        if let Err(err) = saved {
            tracing::warn!(?err, "mode d'outil applique mais non enregistre");
        }
        tracing::info!(server, tool, ?mode, "mode d'un outil externe modifie depuis l'UI");
        let _ = self.events_tx.send(self.event());
    }

    /// Confirmations en attente, pour l'instantane envoye a un client qui se connecte.
    pub fn pending_events(&self) -> Vec<ServerEvent> {
        self.approvals
            .lock()
            .unwrap()
            .iter()
            .map(|(id, p)| ServerEvent::ToolApproval {
                id: *id,
                server: p.server.clone(),
                tool: p.tool.clone(),
                arguments: p.arguments.clone(),
                at_ms: p.at_ms,
            })
            .collect()
    }

    /// Tranche une confirmation (bouton de l'UI).
    pub fn resolve_approval(&self, id: u64, approve: bool) -> bool {
        let Some(pending) = self.approvals.lock().unwrap().remove(&id) else { return false };
        let _ = pending.tx.send(approve);
        let _ = self.events_tx.send(ServerEvent::ToolApprovalResolved { id, approved: approve });
        true
    }

    /// Tranche la confirmation en attente la plus ancienne (reponse a la voix).
    /// Retourne `false` s'il n'y en avait aucune.
    pub fn resolve_oldest(&self, approve: bool) -> bool {
        let oldest = self.approvals.lock().unwrap().keys().min().copied();
        oldest.is_some_and(|id| self.resolve_approval(id, approve))
    }

    /// Y a-t-il une ecriture en attente de confirmation ?
    pub fn has_pending(&self) -> bool {
        !self.approvals.lock().unwrap().is_empty()
    }

    /// Appelle un outil externe en appliquant sa regle. `None` si `exposed` n'est pas un
    /// outil externe. `announce` previent l'utilisateur a la voix quand une confirmation est requise.
    pub async fn call(
        &self,
        exposed: &str,
        arguments: Value,
        announce: impl AsyncFnOnce(String),
    ) -> Option<Result<Value, String>> {
        let (server, tool) = self.find(exposed)?;
        let mode = self.mode_of(&server, &tool);
        let name = &server.config.name;

        if mode == ToolMode::Off {
            return Some(Err(format!("l'outil {} de {name} n'est pas autorise", tool.name)));
        }

        if mode == ToolMode::Confirm {
            let id = self.approval_seq.fetch_add(1, Ordering::SeqCst) + 1;
            let mut shown = serde_json::to_string_pretty(&arguments).unwrap_or_default();
            if shown.chars().count() > 800 {
                shown = shown.chars().take(800).collect::<String>() + "…";
            }
            let (tx, rx) = oneshot::channel();
            let at_ms = now_ms();
            self.approvals.lock().unwrap().insert(
                id,
                PendingApproval { server: name.clone(), tool: tool.name.clone(), arguments: shown.clone(), at_ms, tx },
            );
            let _ = self.events_tx.send(ServerEvent::ToolApproval {
                id,
                server: name.clone(),
                tool: tool.name.clone(),
                arguments: shown,
                at_ms,
            });
            tracing::info!(server = %name, tool = %tool.name, id, "ecriture externe en attente de confirmation");
            announce(format!("une action sur {name} attend votre accord : {}. Confirmez-vous ?", tool.name.replace('_', " ")))
                .await;

            let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
                Ok(Ok(approved)) => approved,
                _ => {
                    // Delai depasse (ou confirmation abandonnee) : refus.
                    if self.approvals.lock().unwrap().remove(&id).is_some() {
                        let _ = self.events_tx.send(ServerEvent::ToolApprovalResolved { id, approved: false });
                    }
                    false
                }
            };
            tracing::info!(server = %name, tool = %tool.name, id, approved, "confirmation d'ecriture externe tranchee");
            if !approved {
                return Some(Ok(json!({
                    "content": [{ "type": "text", "text": "Action refusee par l'utilisateur (ou sans reponse). Ne la retente pas sans nouvelle demande de sa part." }],
                    "isError": true,
                })));
            }
        }

        let upstream = server.upstream.read().unwrap().clone();
        let Some(upstream) = upstream else {
            return Some(Err(format!("le connecteur {name} n'est pas connecte")));
        };
        tracing::info!(server = %name, tool = %tool.name, ?mode, "appel d'outil externe relaye");
        Some(
            upstream
                .request("tools/call", json!({ "name": tool.name, "arguments": arguments }))
                .await
                .map_err(|err| format!("erreur du connecteur {name} : {err:#}")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_d_environnement_dans_les_secrets() {
        // SAFETY: test mono-thread sur une variable propre a ce test.
        unsafe { std::env::set_var("NESTOR_TEST_SECRET", "abc") };
        assert_eq!(expand_env("Bearer ${NESTOR_TEST_SECRET}"), "Bearer abc");
        assert_eq!(expand_env("${NESTOR_TEST_ABSENTE}x"), "x");
        assert_eq!(expand_env("sans variable"), "sans variable");
        assert_eq!(expand_env("accolade ${ouverte"), "accolade ${ouverte");
    }

    #[test]
    fn nom_expose_valide_pour_mcp() {
        assert_eq!(exposed_name("agenda", "list.events"), "agenda__list_events");
        assert!(exposed_name("serveur", &"x".repeat(100)).len() <= 64);
    }

    #[test]
    fn mode_par_defaut_selon_l_annonce_du_serveur() {
        let listing = json!({ "tools": [
            { "name": "lire", "description": "d", "annotations": { "readOnlyHint": true } },
            { "name": "ecrire", "inputSchema": { "type": "object" } },
            { "description": "sans nom" },
        ]});
        let tools = parse_tools(&listing);
        assert_eq!(tools.len(), 2);
        assert_eq!(default_mode(&tools[0]), ToolMode::Read);
        // Sans annonce de lecture seule, on exige une confirmation.
        assert_eq!(default_mode(&tools[1]), ToolMode::Confirm);
    }

    #[test]
    fn reponse_dans_un_flux_sse() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n\n";
        assert_eq!(parse_sse_reply(sse, 7).unwrap()["result"]["ok"], true);
        assert!(parse_sse_reply(sse, 8).is_err());
    }

    /// Passerelle reliee au serveur de test `tests/fixtures/mcp_notes_server.py`.
    async fn gateway(notes: &std::path::Path, auth: bool) -> Arc<Connectors> {
        let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mcp_notes_server.py");
        let config = McpServerConfig {
            name: "carnet".to_string(),
            command: Some("python3".to_string()),
            args: vec![fixture.to_string()],
            env: HashMap::from([("NOTES_FILE".to_string(), notes.display().to_string())]),
            tools: HashMap::from([("tout_effacer".to_string(), ToolMode::Off)]),
            ..Default::default()
        };
        let (events_tx, _) = broadcast::channel(64);
        let overrides = notes.with_extension("overrides.toml");
        let _ = std::fs::remove_file(&overrides);
        let connectors = Connectors::build(&[config], auth, events_tx, overrides);
        if auth {
            let server = connectors.servers[0].clone();
            connectors.connect(&server).await;
        }
        connectors
    }

    fn notes_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nestord-test-{name}-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn text_of(result: &Value) -> String {
        result["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    /// Tranche la confirmation des qu'elle apparait.
    fn answer_when_asked(connectors: &Arc<Connectors>, approve: bool) {
        let connectors = connectors.clone();
        tokio::spawn(async move {
            for _ in 0..200 {
                if connectors.has_pending() {
                    connectors.resolve_oldest(approve);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    }

    #[tokio::test]
    async fn passerelle_applique_les_trois_regles() {
        let notes = notes_path("regles");
        let gw = gateway(&notes, true).await;

        // Liste blanche : l'outil en mode `off` n'est pas expose.
        let exposed: Vec<String> =
            gw.tool_definitions().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(exposed, ["carnet__lire_notes", "carnet__ajouter_note"]);

        // Lecture libre : aucune confirmation demandee.
        let read = gw.call("carnet__lire_notes", json!({}), async |_| panic!("lecture sans confirmation")).await;
        assert_eq!(text_of(&read.unwrap().unwrap()), "(carnet vide)");

        // Ecriture refusee : rien n'est ecrit, et l'assistant est prevenu.
        answer_when_asked(&gw, false);
        let refused = gw.call("carnet__ajouter_note", json!({ "texte": "refusee" }), async |_| {}).await.unwrap().unwrap();
        assert_eq!(refused["isError"], true);
        assert!(!notes.exists(), "une ecriture refusee ne doit rien ecrire");

        // Ecriture approuvee : relayee.
        answer_when_asked(&gw, true);
        let mut announced = false;
        let approved = gw
            .call("carnet__ajouter_note", json!({ "texte": "acheter du pain" }), async |_| announced = true)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(text_of(&approved), "note ajoutee");
        assert!(announced, "l'utilisateur doit etre prevenu a la voix");
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), "acheter du pain\n");

        // Outil masque : refuse meme si l'assistant devine son nom.
        let hidden = gw.call("carnet__tout_effacer", json!({}), async |_| {}).await.unwrap();
        assert!(hidden.is_err());
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), "acheter du pain\n");

        // Un nom qui n'est pas un outil externe est laisse au serveur MCP interne.
        assert!(gw.call("start_mission", json!({}), async |_| {}).await.is_none());
        let _ = std::fs::remove_file(&notes);
    }

    #[tokio::test]
    async fn mode_change_depuis_l_ui() {
        let notes = notes_path("modes");
        let gw = gateway(&notes, true).await;

        // L'ecriture passe en lecture libre : plus de confirmation.
        gw.set_tool_mode("carnet", "ajouter_note", ToolMode::Read);
        let direct = gw.call("carnet__ajouter_note", json!({ "texte": "direct" }), async |_| panic!("plus de confirmation")).await;
        assert_eq!(text_of(&direct.unwrap().unwrap()), "note ajoutee");

        // La lecture est masquee : elle disparait de la liste et n'est plus appelable.
        gw.set_tool_mode("carnet", "lire_notes", ToolMode::Off);
        assert!(!gw.tool_definitions().iter().any(|t| t["name"] == "carnet__lire_notes"));
        assert!(gw.call("carnet__lire_notes", json!({}), async |_| {}).await.unwrap().is_err());

        // Un outil inconnu n'est pas enregistre.
        gw.set_tool_mode("carnet", "inexistant", ToolMode::Read);
        let saved = std::fs::read_to_string(notes.with_extension("overrides.toml")).unwrap();
        assert!(saved.contains("ajouter_note") && !saved.contains("inexistant"));
        let _ = std::fs::remove_file(&notes);
        let _ = std::fs::remove_file(notes.with_extension("overrides.toml"));
    }

    #[tokio::test]
    async fn sans_jeton_aucun_connecteur_externe() {
        let notes = notes_path("sans-jeton");
        let gw = gateway(&notes, false).await;
        assert!(gw.tool_definitions().is_empty());
        assert!(gw.call("carnet__lire_notes", json!({}), async |_| {}).await.is_none());
        let ServerEvent::Connectors { items } = gw.event() else { panic!("evenement inattendu") };
        assert_eq!(items[1].status, "disabled");
    }

    #[test]
    fn configuration_toml_d_un_serveur() {
        let config: McpServerConfig = toml::from_str(
            "name = \"agenda\"\nurl = \"https://exemple.net/mcp\"\n[headers]\nAuthorization = \"Bearer ${X}\"\n[tools]\ncreer = \"confirm\"\nsupprimer = \"off\"\n",
        )
        .unwrap();
        assert_eq!(config.tools["supprimer"], ToolMode::Off);
        assert_eq!(config.tools["creer"], ToolMode::Confirm);
        assert!(config.command.is_none());
    }
}
