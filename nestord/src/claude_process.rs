//! Pilotage headless du sous-processus `claude` (Claude Code CLI).
//!
//! Spawn en mode stream-json bidirectionnel, avec purge stricte des variables
//! d'environnement lieas a une cle API pour forcer la consommation du quota
//! de la session locale (`claude auth login`) plutot qu'une facturation API.

use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, mpsc};

use crate::protocol::{DaemonStatus, Role, ServerEvent, ToolCallStatus};
use crate::usage::UsageState;

/// Variables d'environnement a purger avant le spawn : leur presence ferait
/// basculer le CLI sur la facturation API au lieu du quota de session locale.
const ENV_VARS_TO_SCRUB: &[&str] = &["ANTHROPIC_API_KEY", "CLAUDE_CODE_API_KEY", "CLAUDECODE"];

/// Conditionnement de Claude pour l'usage vocal : sans ca, les reponses sont
/// longues et structurees en markdown, ce qui donne une synthese interminable
/// et hachee a l'ecoute.
const VOICE_SYSTEM_PROMPT: &str = "\
Tu es Nestor, un assistant vocal : tes reponses sont lues a voix haute.
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
se repondent directement sans mission.";

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

/// Spawn le sous-processus `claude -p` en mode headless stream-json et lance
/// les taches d'IO associees. Les evenements produits (deltas de texte,
/// appels d'outils) sont diffuses sur `events_tx` au format `ServerEvent`.
pub fn spawn(
    events_tx: broadcast::Sender<ServerEvent>,
    tts_tx: Option<mpsc::UnboundedSender<String>>,
    usage: Arc<UsageState>,
    mcp_config_path: Option<&std::path::Path>,
) -> Result<ClaudeHandle> {
    let mut cmd = Command::new("claude");
    cmd.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--dangerously-skip-permissions",
        // Requis par le CLI : --output-format stream-json en mode --print impose --verbose.
        "--verbose",
        // Nestor est une interface vocale : les reponses doivent etre courtes et
        // dictables a l'oral, sinon la synthese devient interminable et hachee.
        "--append-system-prompt",
        VOICE_SYSTEM_PROMPT,
    ]);

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
    tokio::spawn(reader_task(stdout, events_tx.clone(), tts_tx, usage));

    // Tache de lecture stderr : simple relais vers les logs.
    tokio::spawn(stderr_task(stderr));

    // Supervision : attend la fin du processus pour logguer le code de sortie.
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => tracing::warn!(?status, "le processus claude s'est termine"),
            Err(err) => tracing::error!(?err, "erreur en attendant la fin du processus claude"),
        }
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

async fn stderr_task(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::warn!(target: "claude::stderr", "{line}");
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

        handle_stream_value(&value, &mut acc, &events_tx, tts_tx.as_ref(), &usage);
    }
}

/// Parse une valeur JSON issue du stdout de `claude` et emet les
/// `ServerEvent` correspondants (deltas texte, appels d'outils).
///
/// Schema reel du CLI (verifie empiriquement, cf. commentaire sur
/// `ClaudeHandle::send_user_message`) :
/// - `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}}`
///   pour le streaming caractere par caractere.
/// - `{"type":"assistant","message":{"content":[{"type":"tool_use","id":...,"name":...,"input":...}, ...]}}`
///   emis une fois le message assistant complet, avec l'input d'outil finalise.
/// - `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":...}, ...]}}`
///   pour le resultat d'execution d'un outil (retour de Claude Code lui-meme, pas l'utilisateur humain).
/// - `{"type":"result", ...}` en toute fin de tour (apres eventuelles boucles d'outils).
fn handle_stream_value(
    value: &Value,
    acc: &mut TurnState,
    events_tx: &broadcast::Sender<ServerEvent>,
    tts_tx: Option<&mpsc::UnboundedSender<String>>,
    usage: &UsageState,
) {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();

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
