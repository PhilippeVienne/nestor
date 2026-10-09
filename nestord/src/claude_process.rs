//! Pilotage headless du sous-processus `claude` (Claude Code CLI).
//!
//! Spawn en mode stream-json bidirectionnel, avec purge stricte des variables
//! d'environnement lieas a une cle API pour forcer la consommation du quota
//! de la session locale (`claude auth login`) plutot qu'une facturation API.
//!
//! La session est remplacable (`ClaudeSlot`) : a la mort du processus ou a la
//! sortie de veille, le superviseur la relance (`ensure_alive`), en reprenant
//! la conversation par `--resume` quand l'identifiant de session est connu.

use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, mpsc};

use crate::config::Config;
use crate::protocol::{DaemonStatus, Role, ServerEvent, ToolCallStatus};
use crate::usage::UsageState;

/// Variables d'environnement a purger avant le spawn : leur presence ferait
/// basculer le CLI sur la facturation API au lieu du quota de session locale.
const ENV_VARS_TO_SCRUB: &[&str] = &["ANTHROPIC_API_KEY", "CLAUDE_CODE_API_KEY", "CLAUDECODE"];

/// Personnalite de Nestor (cf. `.agent/VISION.md`) : elle se loge dans le
/// choix des mots, jamais dans la longueur - les contraintes vocales qui
/// suivent restent prioritaires.
const PERSONALITY_PROMPT: &str = "\
Tu es Nestor, majordome a l'ancienne au service de {address}, dans l'esprit du \
majordome de Moulinsart : style, devoue, imperturbable, avec un humour \
pince-sans-rire discret. Tu vouvoies {address}. Tes deux missions : faire \
avancer ses projets, et le prevenir sans detour des que quelque chose cloche \
ou risque de clocher (retard, oubli, echec, urgence). Tu restes bref : le \
style est dans le choix des mots, jamais dans la longueur. Pas de formules \
pompeuses repetees, pas de citations de l'album.
";

/// Conditionnement de Claude pour l'usage vocal : sans ca, les reponses sont
/// longues et structurees en markdown, ce qui donne une synthese interminable
/// et hachee a l'ecoute.
const VOICE_SYSTEM_PROMPT: &str = "\
Tes reponses sont lues a voix haute.
Contraintes strictes :
- Reponds en 1 a 3 phrases maximum, en francais parle et naturel.
- Jamais de markdown, de listes, de titres, de blocs de code ni d'emoji : \
uniquement des phrases que l'on peut prononcer.
- Va droit au but, pas de preambule ni de recapitulatif de la question.
- Si une information manque, pose une seule question courte.

Delegation : pour toute demande qui depasse quelques secondes de travail \
(ecrire ou lire du code, explorer des fichiers, lancer des tests, faire une \
recherche), n'execute pas toi-meme. Appelle l'outil start_mission avec une \
description autonome de la tache, puis annonce en une phrase que c'est lance. \
Le sous-agent travaille en arriere-plan et son compte rendu te reviendra plus \
tard sous la forme d'un rapport interne : tu l'annonceras alors brievement. \
Reste disponible pour parler pendant ce temps. Les questions simples, elles, \
se repondent directement sans mission.

Memoire de taches : des que {address} mentionne quelque chose a faire, a \
retenir, ou une habitude a prendre, appelle todo_add sans demander de \
confirmation superflue - une tache oubliee est pire qu'une tache retenue a \
tort. Utilise todo_list pour verifier avant d'en ajouter une similaire ou \
quand on te demande ou ca en est, et todo_complete des que {address} dit \
avoir fait quelque chose qui y correspond. Un « [Rappel interne : ... ]» \
qui arrive de lui-meme dans la conversation est une relance a faire \
naturellement, jamais une liste recitee. Quand le contexte s'y prete \
(fin d'une tache, moment calme), tu peux aussi prendre l'initiative de \
proposer la suite d'un projet en cours ou une tache en retard, en une \
phrase, sans insister si {address} n'y donne pas suite.";

/// Assemble le prompt systeme : personnalite d'abord, contraintes vocales et
/// regle de delegation ensuite. La forme d'adresse vient de la configuration
/// (`config.toml`, ou `NESTORD_ADDRESS_FORM` qui a priorite dessus).
fn build_system_prompt(config: &Config) -> String {
    format!(
        "{}\n{VOICE_SYSTEM_PROMPT}",
        PERSONALITY_PROMPT.replace("{address}", &config.address_form)
    )
}

/// Poignee permettant d'envoyer du texte utilisateur vers le sous-processus `claude`.
#[derive(Clone)]
pub struct ClaudeHandle {
    stdin_tx: mpsc::Sender<String>,
}

impl ClaudeHandle {
    /// Envoie un message utilisateur au processus `claude` sous forme NDJSON.
    ///
    /// Le schema attendu par le CLI en `--input-format stream-json` reprend
    /// la forme d'un message de la Messages API (verifie empiriquement, car
    /// non documente par `claude -p --help`) :
    /// `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]}}`.
    pub async fn send_user_message(&self, content: &str) -> Result<()> {
        let line = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": content }],
            },
        })
        .to_string();
        self.stdin_tx
            .send(line)
            .await
            .context("le sous-processus claude n'est plus joignable")
    }

    /// Variante bloquante de [`Self::send_user_message`], pour un appel depuis
    /// un thread synchrone (ex: la boucle d'ecoute audio, hors executeur tokio).
    #[allow(dead_code)]
    pub fn send_user_message_blocking(&self, content: &str) -> Result<()> {
        let line = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": content }],
            },
        })
        .to_string();
        self.stdin_tx.blocking_send(line).context("le sous-processus claude n'est plus joignable")
    }
}

/// Emplacement de la session conversationnelle courante. Vide tant que le
/// processus n'est pas lance, et apres sa mort jusqu'a la relance : la session
/// est remplacable, ce qui permet de la relancer apres un plantage ou une
/// sortie de veille (cf. [`ensure_alive`]).
#[derive(Default)]
pub struct ClaudeSlot {
    inner: std::sync::RwLock<Option<ClaudeHandle>>,
}

impl ClaudeSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<ClaudeHandle> {
        self.inner.read().unwrap().clone()
    }

    pub fn set(&self, handle: ClaudeHandle) {
        *self.inner.write().unwrap() = Some(handle);
    }

    pub fn clear(&self) {
        *self.inner.write().unwrap() = None;
    }

    pub fn is_alive(&self) -> bool {
        self.inner.read().unwrap().is_some()
    }
}

/// Tout ce qu'il faut pour (re)lancer la session.
#[derive(Clone)]
pub struct SpawnParams {
    pub events_tx: broadcast::Sender<ServerEvent>,
    pub tts_tx: Option<mpsc::UnboundedSender<String>>,
    pub usage: Arc<UsageState>,
    pub mcp_config_path: Option<std::path::PathBuf>,
    pub brain: Arc<crate::brain::NestorBrain>,
    pub config: Arc<Config>,
    pub slot: Arc<ClaudeSlot>,
}

/// Superviseur de la session : parametres de lancement, identifiant de session
/// pour reprendre la conversation (`--resume`), et compteur d'echecs pour le
/// delai entre deux relances.
struct Supervisor {
    params: SpawnParams,
    session_id: std::sync::Mutex<Option<String>>,
    relaunching: std::sync::atomic::AtomicBool,
    attempts: std::sync::atomic::AtomicU32,
    /// La derniere reprise par `--resume` est morte aussitot : la suivante repart a neuf.
    resume_failed: std::sync::atomic::AtomicBool,
}

static SUPERVISOR: std::sync::OnceLock<Supervisor> = std::sync::OnceLock::new();

/// Enregistre les parametres de lancement : a appeler une fois, avant le premier `spawn`.
pub fn install(params: SpawnParams) {
    let _ = SUPERVISOR.set(Supervisor {
        params,
        session_id: std::sync::Mutex::new(None),
        relaunching: std::sync::atomic::AtomicBool::new(false),
        attempts: std::sync::atomic::AtomicU32::new(0),
        resume_failed: std::sync::atomic::AtomicBool::new(false),
    });
}

/// Delai avant la relance numero `attempts` (0 : immediate), plafonne a cinq minutes.
pub fn relaunch_delay(attempts: u32) -> std::time::Duration {
    if attempts == 0 {
        return std::time::Duration::ZERO;
    }
    std::time::Duration::from_secs((5u64 << (attempts - 1).min(10)).min(300))
}

/// Relance la session si elle est morte (sans effet si elle tourne ou si une
/// relance est deja en cours). Appele par la supervision du processus et a la
/// sortie de veille (`power.rs`). La conversation reprend avec `--resume` quand
/// l'identifiant de session est connu ; si cette reprise echoue, la tentative
/// suivante repart d'une session neuve.
pub fn ensure_alive() {
    let Some(supervisor) = SUPERVISOR.get() else { return };
    if supervisor.params.slot.is_alive() {
        return;
    }
    if supervisor.relaunching.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        use std::sync::atomic::Ordering;
        let attempts = supervisor.attempts.load(Ordering::SeqCst);
        let delay = relaunch_delay(attempts);
        if !delay.is_zero() {
            tracing::info!(?delay, attempts, "relance de la session claude programmee");
            tokio::time::sleep(delay).await;
        }
        // Reprise de la conversation, sauf si la reprise precedente est morte aussitot
        // (identifiant invalide ou session corrompue) : on repart alors a neuf.
        let resume = if supervisor.resume_failed.load(Ordering::SeqCst) {
            None
        } else {
            supervisor.session_id.lock().unwrap().clone()
        };
        match spawn(&supervisor.params, resume.as_deref()) {
            Ok(handle) => {
                // Poignee stockee et drapeau rendu avant le moindre `await` : si le
                // processus meurt aussitot, sa supervision peut relancer a son tour.
                supervisor.params.slot.set(handle);
                supervisor.relaunching.store(false, Ordering::SeqCst);
                tracing::info!(resumed = resume.is_some(), "session claude relancee");
                // Retour a Claude si le mode reduit ne tenait qu'a la mort du processus,
                // jamais par-dessus un choix manuel de l'utilisateur.
                if !supervisor.params.brain.fallback_is_manual() {
                    supervisor.params.brain.set_backend("auto").await;
                }
                crate::sleep::on_session_ready(&supervisor.params.brain).await;
            }
            Err(err) => {
                tracing::error!(?err, "relance de la session claude impossible");
                supervisor.attempts.fetch_add(1, Ordering::SeqCst);
                supervisor.relaunching.store(false, Ordering::SeqCst);
                ensure_alive();
            }
        }
    });
}

/// La session conversationnelle est-elle lancee ?
pub fn session_alive() -> bool {
    SUPERVISOR.get().is_some_and(|s| s.params.slot.is_alive())
}

fn remember_session(session_id: &str) {
    if let Some(supervisor) = SUPERVISOR.get() {
        let mut current = supervisor.session_id.lock().unwrap();
        if current.as_deref() != Some(session_id) {
            tracing::debug!(session_id, "identifiant de session claude retenu");
            *current = Some(session_id.to_string());
        }
    }
}

/// Spawn le sous-processus `claude -p` en mode headless stream-json et lance
/// les taches d'IO associees. Les evenements produits (deltas de texte,
/// appels d'outils) sont diffuses sur `events_tx` au format `ServerEvent`.
/// `resume` : identifiant d'une session precedente a reprendre.
pub fn spawn(params: &SpawnParams, resume: Option<&str>) -> Result<ClaudeHandle> {
    let SpawnParams { events_tx, tts_tx, usage, mcp_config_path, brain, config, slot } = params.clone();
    let mcp_config_path = mcp_config_path.as_deref();
    let mut cmd = Command::new("claude");
    if let Some(session_id) = resume {
        cmd.arg("--resume").arg(session_id);
    }
    cmd.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        // Mode auto du CLI plutot que `--dangerously-skip-permissions` : personne ne peut
        // repondre a une invite ici, donc un classifieur decide action par action. Les
        // lectures et les modifications du dossier de travail passent sans examen ; le reste
        // (shell, reseau, outils MCP qui ne sont pas en lecture seule) est examine et peut
        // etre refuse, auquel cas l'assistant recoit le motif et poursuit son tour. C'est un
        // filet, pas une garantie : une commande destructrice peut passer, et le mode exige
        // un modele et un compte qui le proposent (sinon le CLI demarre en mode manuel, ou
        // toute action soumise a invite est refusee faute d'interlocuteur).
        "--permission-mode",
        "auto",
        // Requis par le CLI : --output-format stream-json en mode --print impose --verbose.
        "--verbose",
        // Seul le serveur MCP de nestord est visible : sans cela, les connecteurs attaches au
        // compte (Gmail, Drive...) seraient utilisables directement, ecritures comprises, sans
        // passer par la passerelle et sa confirmation (cf. `connectors.rs`).
        "--strict-mcp-config",
        // Sans les reglages ni les consignes personnelles de l'utilisateur (`~/.claude/`) :
        // elles sont ecrites pour ses sessions de travail au clavier (par exemple « terminer
        // chaque tour par une question a choix »), pas pour un majordome vocal qui les
        // reciterait. Les consignes du projet courant restent chargees.
        "--setting-sources",
        "project,local",
    ]);
    // Personnalite de majordome + contraintes vocales : sans ca, les reponses
    // sont longues et en markdown, donc interminables a l'ecoute.
    cmd.arg("--append-system-prompt").arg(build_system_prompt(&config));

    // Serveur MCP de nestord : expose l'outil de delegation de mission. Le
    // serveur HTTP doit deja ecouter, le CLI s'y connecte au demarrage de session.
    if let Some(path) = mcp_config_path {
        cmd.arg("--mcp-config").arg(path);
    }

    // Scrub strict : on herite de l'environnement courant puis on retire
    // explicitement les cles API pour forcer l'usage du quota de session locale.
    for var in ENV_VARS_TO_SCRUB {
        cmd.env_remove(var);
    }
    // Secrets que l'assistant n'a pas a connaitre : jeton de nestord, identifiants des connecteurs.
    for var in crate::auth::secret_env_vars() {
        cmd.env_remove(var);
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child: Child = cmd.spawn().context("impossible de spawn le processus claude")?;

    let stdin = child.stdin.take().context("stdin du processus claude indisponible")?;
    let stdout = child.stdout.take().context("stdout du processus claude indisponible")?;
    let stderr = child.stderr.take().context("stderr du processus claude indisponible")?;

    let (stdin_tx, stdin_rx) = mpsc::channel::<String>(64);

    // Tache d'ecriture : forward des messages NDJSON vers stdin du sous-processus.
    tokio::spawn(writer_task(stdin, stdin_rx));

    // Tache de lecture : parsing au fil de l'eau du stdout (stream_event/text_delta).
    tokio::spawn(reader_task(stdout, events_tx.clone(), tts_tx, usage, brain.clone()));

    // Tache de lecture stderr : simple relais vers les logs et detection d'erreur de quota.
    tokio::spawn(stderr_task(stderr, brain.clone()));

    // Supervision : attend la fin du processus, bascule en mode reduit le temps de
    // relancer, et programme la relance (delai croissant si elle echoue en boucle).
    let brain_sup = brain.clone();
    let started = std::time::Instant::now();
    let resumed = resume.is_some();
    tokio::spawn(async move {
        let reason = match child.wait().await {
            Ok(status) => {
                tracing::warn!(?status, "le processus claude s'est termine");
                "Processus Claude termine, relance en cours"
            }
            Err(err) => {
                tracing::error!(?err, "erreur en attendant la fin du processus claude");
                "Erreur processus Claude, relance en cours"
            }
        };
        // L'emplacement est vide avant tout : personne ne doit plus voir cette poignee.
        slot.clear();
        // Bascule silencieuse : ce n'est pas un quota epuise, et la relance suit.
        brain_sup.trigger_fallback_quiet(reason).await;
        if let Some(supervisor) = SUPERVISOR.get() {
            use std::sync::atomic::Ordering;
            // Mort precoce : la relance precedente n'a pas tenu, on espace les suivantes.
            // Une session qui a tourne un moment repart sans attendre.
            if started.elapsed() < std::time::Duration::from_secs(30) {
                supervisor.attempts.fetch_add(1, Ordering::SeqCst);
                if resumed {
                    supervisor.resume_failed.store(true, Ordering::SeqCst);
                }
            } else {
                supervisor.attempts.store(0, Ordering::SeqCst);
                supervisor.resume_failed.store(false, Ordering::SeqCst);
            }
        }
        ensure_alive();
    });

    Ok(ClaudeHandle { stdin_tx })
}

async fn writer_task(mut stdin: ChildStdin, mut rx: mpsc::Receiver<String>) {
    while let Some(mut line) = rx.recv().await {
        line.push('\n');
        if let Err(err) = stdin.write_all(line.as_bytes()).await {
            tracing::error!(?err, "echec d'ecriture sur stdin du processus claude");
            break;
        }
        if let Err(err) = stdin.flush().await {
            tracing::error!(?err, "echec de flush sur stdin du processus claude");
            break;
        }
    }
}

async fn stderr_task(stderr: tokio::process::ChildStderr, brain: Arc<crate::brain::NestorBrain>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::warn!(target: "claude::stderr", "{line}");
        if line.contains("session limit") || line.contains("Credit balance is too low") || line.contains("rate limit") {
            brain.trigger_fallback("Session Claude limitee (stderr)").await;
        }
    }
}

/// Etat d'accumulation du flux texte assistant courant (pour le decoupage
/// par ponctuation avant envoi au pipeline TTS) et suivi des appels d'outils
/// en cours (pour associer un `tool_result` a son `tool_use` d'origine).
#[derive(Default)]
struct TurnState {
    full_text: String,
    sentence_buf: String,
    pending_tools: std::collections::HashMap<String, (String, Value)>,
}

/// Ponctuations de fin de phrase uniquement : couper sur les virgules
/// fragmentait la synthese en bouts de quelques mots, chacun synthetise
/// separement avec sa propre prosodie - a l'ecoute, le resultat etait hache.
const SENTENCE_BOUNDARIES: &[char] = &['.', '!', '?', '\n'];

/// Longueur minimale d'un segment envoye au TTS. En dessous, on continue
/// d'accumuler (le reliquat est de toute facon flushe en fin de tour), ce qui
/// evite de synthetiser des fragments trop courts pour porter une intonation.
const MIN_TTS_SEGMENT_CHARS: usize = 40;

impl TurnState {
    /// Ajoute un delta de texte. Retourne les segments de phrase completes
    /// (a envoyer immediatement au TTS) produits par ce delta.
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

            // Segment encore trop court : on attend la prochaine ponctuation
            // plutot que d'emettre un fragment isole.
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

/// Retire les artefacts markdown que la synthese prononcerait litteralement
/// (backticks, emphases, puces, titres). Retourne `None` s'il ne reste rien
/// de prononcable.
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

async fn reader_task(
    stdout: tokio::process::ChildStdout,
    events_tx: broadcast::Sender<ServerEvent>,
    tts_tx: Option<mpsc::UnboundedSender<String>>,
    usage: Arc<UsageState>,
    brain: Arc<crate::brain::NestorBrain>,
) {
    let mut lines = BufReader::new(stdout).lines();
    let mut acc = TurnState::default();

    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(err) => {
                tracing::error!(?err, "erreur de lecture sur stdout du processus claude");
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                tracing::debug!(?err, raw = %line, "ligne stdout non-JSON ignoree");
                continue;
            }
        };

        handle_stream_value(&value, &mut acc, &events_tx, tts_tx.as_ref(), &usage, &brain).await;
    }
}

/// Parse une valeur JSON issue du stdout de `claude` et emet les
/// `ServerEvent` correspondants (deltas texte, appels d'outils).
async fn handle_stream_value(
    value: &Value,
    acc: &mut TurnState,
    events_tx: &broadcast::Sender<ServerEvent>,
    tts_tx: Option<&mpsc::UnboundedSender<String>>,
    usage: &UsageState,
    brain: &Arc<crate::brain::NestorBrain>,
) {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();

    // Identifiant de session, pour reprendre la conversation apres une relance.
    if let Some(session_id) = value.get("session_id").and_then(Value::as_str) {
        remember_session(session_id);
    }

    // Detection immediate des erreurs de quota ou rate limit de session
    let is_err = value.get("is_error").and_then(Value::as_bool).unwrap_or(false)
        || value.get("api_error_status").and_then(Value::as_i64) == Some(429)
        || value.pointer("/message/error").and_then(Value::as_str) == Some("rate_limit");

    if is_err {
        let msg = value
            .get("result")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/message/content/0/text").and_then(Value::as_str))
            .unwrap_or("Quota de session Claude atteint");
        tracing::warn!(msg, "Erreur de quota Claude interceptee");
        usage.set_exhausted(true);
        brain.trigger_fallback(msg).await;
    }

    match event_type {
        "stream_event" => {
            let Some(event) = value.get("event") else { return };
            let inner_type = event.get("type").and_then(Value::as_str).unwrap_or_default();

            if inner_type == "content_block_delta" {
                let delta_text = event
                    .get("delta")
                    .filter(|d| d.get("type").and_then(Value::as_str) == Some("text_delta"))
                    .and_then(|d| d.get("text"))
                    .and_then(Value::as_str);

                if let Some(delta) = delta_text {
                    brain.clear_pending_user_message();
                    let sentences = acc.push_delta(delta);
                    let _ = events_tx.send(ServerEvent::Transcript {
                        role: Role::Assistant,
                        delta: Some(delta.to_string()),
                        text: acc.full_text.clone(),
                        is_final: Some(false),
                    });
                    for sentence in sentences {
                        if let Some(tts_tx) = tts_tx {
                            let _ = tts_tx.send(sentence);
                        }
                    }
                }
            } else if inner_type == "message_start" && !acc.full_text.is_empty() {
                // Un nouveau message assistant demarre au sein du meme tour
                // (typiquement apres un aller-retour d'outil) : on separe visuellement.
                acc.full_text.push('\n');
            }
        }
        "assistant" => {
            let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else { return };
            for block in content {
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                let id = block.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let name = block.get("name").and_then(Value::as_str).unwrap_or("unknown").to_string();
                let input = block.get("input").cloned().unwrap_or(Value::Null);

                let _ = events_tx.send(ServerEvent::ToolCall {
                    name: name.clone(),
                    input: input.clone(),
                    status: ToolCallStatus::Running,
                    mission_id: None,
                });
                acc.pending_tools.insert(id, (name, input));
            }
        }
        "user" => {
            let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else { return };
            for block in content {
                if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else { continue };
                let (name, input) = acc
                    .pending_tools
                    .remove(tool_use_id)
                    .unwrap_or_else(|| ("unknown".to_string(), Value::Null));

                let _ = events_tx.send(ServerEvent::ToolCall {
                    name,
                    input,
                    status: ToolCallStatus::Completed,
                    mission_id: None,
                });
            }
        }
        "result" => {
            // Si la reponse etait vide (ex: crash ou rejet de quota Claude),
            // on ignore pour ne pas polluer l'UI avec un tour vide.
            if acc.full_text.is_empty() {
                acc.sentence_buf.clear();
                acc.pending_tools.clear();
                return;
            }

            // Flush du reliquat (derniere phrase sans ponctuation finale, ou
            // segment reste sous MIN_TTS_SEGMENT_CHARS) vers le TTS, sinon il
            // ne serait jamais prononce.
            if let (Some(tts_tx), Some(leftover)) = (tts_tx, sanitize_for_tts(&acc.sentence_buf)) {
                let _ = tts_tx.send(leftover);
            }

            let _ = events_tx.send(ServerEvent::Transcript {
                role: Role::Assistant,
                delta: None,
                text: acc.full_text.clone(),
                is_final: Some(true),
            });
            let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Idle });
            acc.full_text.clear();
            acc.sentence_buf.clear();
            acc.pending_tools.clear();
        }
        "rate_limit_event" => {
            if let Some((five_hour, seven_day, resets_at)) = usage.update_from_event(value) {
                tracing::debug!(five_hour, seven_day, "quota de session mis a jour");
                let _ = events_tx.send(ServerEvent::Usage { five_hour, seven_day, resets_at });
            }
        }
        _ => {
            tracing::trace!(%event_type, "evenement stream ignore");
        }
    }
}

#[cfg(test)]
mod supervision_tests {
    use super::*;

    #[test]
    fn delai_de_relance_croissant_et_plafonne() {
        assert!(relaunch_delay(0).is_zero());
        assert_eq!(relaunch_delay(1).as_secs(), 5);
        assert_eq!(relaunch_delay(2).as_secs(), 10);
        assert_eq!(relaunch_delay(4).as_secs(), 40);
        assert_eq!(relaunch_delay(7).as_secs(), 300);
        assert_eq!(relaunch_delay(40).as_secs(), 300, "pas de debordement");
    }

    #[test]
    fn emplacement_de_session_vide_puis_rempli() {
        let slot = ClaudeSlot::new();
        assert!(!slot.is_alive());
        let (tx, _rx) = mpsc::channel(1);
        slot.set(ClaudeHandle { stdin_tx: tx });
        assert!(slot.is_alive() && slot.get().is_some());
        slot.clear();
        assert!(slot.get().is_none());
    }
}
