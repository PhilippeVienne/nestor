//! Capture des faits par mission de fond (`docs/memory.md` §5).
//!
//! La session vocale ne doit pas s'arreter pour archiver : les tours de
//! conversation (texte de l'utilisateur, reponse finale de Nestor) sont mis en
//! tampon, et quand un lot est mur (assez de tours et un moment de calme, ou
//! un lot plein) un sous-agent `claude` en lit le contenu et en extrait les
//! faits durables, en JSON. nestord les ecrit dans la memoire par la meme
//! regle de dedoublonnage que `memory_write`. Le sous-agent n'a aucun outil :
//! il lit du texte et rend du JSON, rien d'autre.
//!
//! Cout : un appel de modele par lot, sur le modele `[memory] capture_model`
//! (haiku par defaut). Desactivable par `[memory] capture = false`.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::clock::now_ms;
use crate::config::{Config, MemoryConfig};
use crate::memory::{MemoryStore, NewNode, KINDS};
use crate::protocol::{Role, ServerEvent};

const ENV_VARS_TO_SCRUB: &[&str] = &["ANTHROPIC_API_KEY", "CLAUDE_CODE_API_KEY", "CLAUDECODE"];

/// Un tour de conversation retenu pour l'archivage.
#[derive(Debug, Clone)]
pub struct Turn {
    pub role: &'static str,
    pub text: String,
}

/// Fait tel que le sous-agent le rend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExtractedFact {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub tags: String,
    /// Titre d'un fait anterieur que celui-ci contredit.
    #[serde(default)]
    pub replaces_title: Option<String>,
}

/// Le lot est-il mur ? Plein, ou assez fourni et suivi d'un moment de calme.
pub fn should_capture(turns: usize, chars: usize, idle_ms: u64, cfg: &MemoryConfig) -> bool {
    if turns == 0 {
        return false;
    }
    turns >= cfg.max_turns.max(1)
        || chars >= cfg.max_chars.max(500)
        || (turns >= cfg.min_turns.max(1) && idle_ms >= cfg.idle_minutes.max(1) * 60_000)
}

pub fn build_prompt(turns: &[Turn], address: &str) -> String {
    let transcript: Vec<String> = turns
        .iter()
        .map(|t| format!("{} : {}", if t.role == "user" { address } else { "Nestor" }, t.text.trim()))
        .collect();
    format!(
        "Tu es l'archiviste de Nestor, majordome vocal de {address}. Voici des tours de conversation \
entre {address} et Nestor. Extrais uniquement les faits durables qui meritent d'etre retenus : \
preferences, decisions et leur pourquoi, pieges rencontres, informations stables sur {address}, \
projets en cours. Ignore le bavardage, les questions sans suite et les reponses de pure forme. Ne \
retiens jamais une position, un horaire ponctuel, ni le contenu d'un mail ou d'un rendez-vous.\n\
Reponds UNIQUEMENT par un tableau JSON, eventuellement vide, d'objets de la forme \
{{\"kind\": \"person|preference|project|decision|pitfall|code|place|fact\", \"title\": \"une ligne \
prononcable\", \"body\": \"detail, 500 caracteres au plus\", \"context\": \"perso|projet:<nom>|machine:<nom>\", \
\"tags\": \"mot,mot\", \"replaces_title\": \"titre d'un fait anterieur contredit, sinon omis\"}}. \
Aucun texte autour du tableau.\n\n---\n{}\n---",
        transcript.join("\n")
    )
}

/// Tableau JSON dans la reponse, meme entoure de texte ou de barrieres de code.
pub fn parse_facts(output: &str) -> Vec<ExtractedFact> {
    let Some(start) = output.find('[') else { return Vec::new() };
    let Some(end) = output.rfind(']') else { return Vec::new() };
    if end <= start {
        return Vec::new();
    }
    let facts: Vec<ExtractedFact> = serde_json::from_str(&output[start..=end]).unwrap_or_default();
    facts
        .into_iter()
        .filter(|f| !f.title.trim().is_empty())
        .map(|mut f| {
            if !KINDS.contains(&f.kind.as_str()) || f.kind == "mission" {
                f.kind = "fact".to_string();
            }
            f
        })
        .collect()
}

/// Ecrit les faits extraits ; retourne (nouveaux, mis a jour).
pub fn apply(store: &MemoryStore, facts: &[ExtractedFact], source: &str) -> (usize, usize) {
    let (mut created, mut updated) = (0, 0);
    for fact in facts {
        let context = if fact.context.trim().is_empty() { "perso" } else { fact.context.trim() };
        let replaces = fact
            .replaces_title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .and_then(|title| store.find_by_title(context, title).ok().flatten());
        let node = NewNode {
            kind: fact.kind.clone(),
            title: fact.title.clone(),
            body: fact.body.clone(),
            context: context.to_string(),
            tags: fact.tags.clone(),
            source: source.to_string(),
        };
        match store.write(node, replaces, &[]) {
            Ok(outcome) if outcome.updated => updated += 1,
            Ok(_) => created += 1,
            Err(err) => tracing::warn!(?err, "fait extrait non ecrit"),
        }
    }
    (created, updated)
}

/// Sous-agent sans outil : lit le lot, rend le JSON. Reponse `result` du CLI.
async fn extract(cfg: &MemoryConfig, prompt: &str) -> Result<String> {
    let mut cmd = tokio::process::Command::new("claude");
    cmd.args(["-p", prompt, "--output-format", "json", "--permission-mode", "auto", "--strict-mcp-config"]);
    cmd.args(["--setting-sources", "project,local"]);
    if !cfg.capture_model.trim().is_empty() {
        cmd.args(["--model", cfg.capture_model.trim()]);
    }
    for var in ENV_VARS_TO_SCRUB {
        cmd.env_remove(var);
    }
    for var in crate::auth::secret_env_vars() {
        cmd.env_remove(var);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().context("lancement de claude pour l'archivage")?;
    let output = tokio::time::timeout(Duration::from_secs(180), child.wait_with_output())
        .await
        .context("archivage : delai depasse")?
        .context("archivage : attente du sous-agent")?;
    anyhow::ensure!(output.status.success(), "archivage : {}", String::from_utf8_lossy(&output.stderr).trim());
    let value: Value = serde_json::from_slice(&output.stdout).context("archivage : sortie illisible")?;
    Ok(value.get("result").and_then(Value::as_str).unwrap_or_default().to_string())
}

/// Lance la capture de fond. Sans effet si `[memory] capture = false`.
pub fn spawn(events_tx: broadcast::Sender<ServerEvent>, config: Arc<Config>) {
    let cfg = config.memory.clone();
    if !cfg.capture {
        tracing::info!("capture des faits de conversation desactivee");
        return;
    }
    let mut events_rx = events_tx.subscribe();
    tokio::spawn(async move {
        let mut turns: Vec<Turn> = Vec::new();
        let mut chars = 0usize;
        let mut last_turn_ms = 0u64;
        let mut running = false;
        let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                event = events_rx.recv() => match event {
                    Ok(ServerEvent::Transcript { role, text, is_final: Some(true), .. }) => {
                        let text = text.trim();
                        // Les rapports internes partent vers l'assistant sans transcription utilisateur ;
                        // ses reponses, elles, sont retenues : elles disent ce qui a ete decide.
                        if text.is_empty() {
                            continue;
                        }
                        let role = if matches!(role, Role::User) { "user" } else { "assistant" };
                        chars += text.chars().count();
                        turns.push(Turn { role, text: text.to_string() });
                        last_turn_ms = now_ms();
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = done_rx.recv() => running = false,
                _ = interval.tick() => {
                    let idle_ms = now_ms().saturating_sub(last_turn_ms);
                    if running || !should_capture(turns.len(), chars, idle_ms, &cfg) {
                        continue;
                    }
                    let Some(store) = crate::memory::global().cloned() else { continue };
                    let batch = std::mem::take(&mut turns);
                    chars = 0;
                    running = true;
                    let prompt = build_prompt(&batch, &config.address_form);
                    let cfg = cfg.clone();
                    let done = done_tx.clone();
                    tokio::spawn(async move {
                        let source = format!("capture:{}", now_ms());
                        match extract(&cfg, &prompt).await {
                            Ok(output) => {
                                let facts = parse_facts(&output);
                                let (created, updated) = apply(&store, &facts, &source);
                                tracing::info!(turns = batch.len(), created, updated, "faits de conversation archives");
                            }
                            Err(err) => tracing::warn!(?err, turns = batch.len(), "archivage de la conversation impossible"),
                        }
                        let _ = done.send(()).await;
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MemoryConfig {
        MemoryConfig::default()
    }

    #[test]
    fn lot_mur_quand_plein_ou_calme() {
        assert!(!should_capture(0, 0, u64::MAX, &cfg()));
        assert!(!should_capture(2, 100, 10 * 60_000, &cfg()), "trop peu de tours");
        assert!(!should_capture(5, 100, 60_000, &cfg()), "pas encore de calme");
        assert!(should_capture(5, 100, 6 * 60_000, &cfg()));
        assert!(should_capture(12, 100, 0, &cfg()), "lot plein");
        assert!(should_capture(1, 7_000, 0, &cfg()), "lot lourd");
    }

    #[test]
    fn faits_lus_malgre_le_texte_autour_et_types_ramenes() {
        let output = "Voici les faits :\n```json\n[{\"kind\": \"preference\", \"title\": \"Pas de markdown a l'oral\"}, \
{\"kind\": \"mission\", \"title\": \"X\", \"body\": \"y\", \"replaces_title\": \"ancien\"}, {\"kind\": \"truc\", \"title\": \"Z\"}, \
{\"kind\": \"fact\", \"title\": \"  \"}]\n```\nFin.";
        let facts = parse_facts(output);
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].kind, "preference");
        assert_eq!(facts[1].kind, "fact", "mission est reserve a la capture des comptes rendus");
        assert_eq!(facts[1].replaces_title.as_deref(), Some("ancien"));
        assert_eq!(facts[2].kind, "fact");
        assert!(parse_facts("rien a retenir").is_empty());
        assert!(parse_facts("[]").is_empty());
        assert!(parse_facts("[pas du json").is_empty());
    }

    #[test]
    fn application_avec_remplacement() {
        let path = std::env::temp_dir().join(format!("nestord-capture-test-{}-{}.db", std::process::id(), now_ms()));
        let _ = std::fs::remove_file(&path);
        let store = MemoryStore::open(&path).unwrap();
        let first = vec![ExtractedFact {
            kind: "preference".into(),
            title: "Reunion le lundi".into(),
            body: String::new(),
            context: "perso".into(),
            tags: String::new(),
            replaces_title: None,
        }];
        assert_eq!(apply(&store, &first, "capture:1"), (1, 0));
        assert_eq!(apply(&store, &first, "capture:2"), (0, 1), "le meme fait est mis a jour");
        let second = vec![ExtractedFact {
            kind: "preference".into(),
            title: "Reunion le mardi".into(),
            body: "depuis octobre".into(),
            context: String::new(),
            tags: "agenda".into(),
            replaces_title: Some("reunion le LUNDI".into()),
        }];
        assert_eq!(apply(&store, &second, "capture:3"), (1, 0));
        let current = store.search("reunion", None, &[], 5, false).unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].title, "Reunion le mardi");
        assert_eq!(current[0].source, "capture:3");
    }

    #[test]
    fn prompt_nomme_les_interlocuteurs() {
        let turns = vec![
            Turn { role: "user", text: "Retiens que je prefere le train.".into() },
            Turn { role: "assistant", text: "C'est note, Monsieur.".into() },
        ];
        let prompt = build_prompt(&turns, "Monsieur");
        assert!(prompt.contains("Monsieur : Retiens que je prefere le train."));
        assert!(prompt.contains("Nestor : C'est note, Monsieur."));
        assert!(prompt.contains("tableau JSON"));
    }
}
