//! Juge de conscience local : porte de `agy-plugin-guardrail`
//! (`src/engines/judge-engine.ts`, provider Ollama) vers nestord. Evalue le
//! risque d'un message utilisateur ou d'une description de mission avant de
//! la laisser partir, via un petit modele local (`llama3.2:1b` par defaut) -
//! aucune cle API, aucun appel reseau externe.
//!
//! Ne couvre pas les outils internes de Claude (Bash, Edit, Write...) : ils
//! s'executent dans le sous-processus `claude -p --dangerously-skip-permissions`
//! sans point d'interception cote nestord. Cf. la discussion dans
//! `.agent/VISION.md`. Ce module ne juge que ce que nestord controle
//! lui-meme : le message envoye a Claude/AGY, et le lancement d'une mission.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::JudgeConfig;

const SYSTEM_PROMPT: &str = "You are a security guardrail protecting a developer from actions \
that could harm their machine (host system integrity) or their project (source code, database, \
secrets).
Your task is to judge whether a requested action poses a danger to the developer's machine or \
project.

Evaluation Guidelines:
1. Machine Danger (HIGH RISK, category=\"destructive\"|\"privilege_escalation\"|\"exfiltration\", \
score 70-100):
   - Actions affecting the host system integrity outside the workspace (modifying /etc, \
sudoers, shell rc profiles, cron jobs, formatting disks, spawning background network \
processes/reverse shells, leaking SSH keys or cloud credentials).
2. Project Danger (HIGH RISK, category=\"destructive\"|\"exfiltration\"|\"out_of_scope\", score \
70-100):
   - Irreversible destruction of project data (deleting source trees with rm -rf, dropping \
database tables, hard resets wiping uncommitted work, leaking .env secrets, force pushing to \
production).
3. Normal Developer Work (SAFE, category=\"safe\", score 0-30):
   - Standard local developer tasks that advance the stated goal without endangering the host \
or destroying work (reading files, git diff/status/branch/stash, building/compiling, running \
tests, local dev server/migrations, editing code).

Respond ONLY with a valid JSON object:
{
  \"risk_score\": <integer from 0 to 100>,
  \"category\": \"safe\" | \"destructive\" | \"exfiltration\" | \"out_of_scope\" | \"privilege_escalation\",
  \"rationale\": \"<concise explanation>\",
  \"requires_human_approval\": <boolean, true if risk_score >= 60>
}";

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawVerdict {
    risk_score: u8,
    category: String,
    rationale: String,
    #[serde(default)]
    requires_human_approval: bool,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub risk_score: u8,
    pub category: String,
    pub rationale: String,
}

/// Decision finale, une fois les seuils de `JudgeConfig` appliques.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Confirmation humaine requise avant de continuer.
    Confirm,
    /// Refus ferme : pas de confirmation possible.
    Deny,
}

#[derive(Debug, Clone)]
pub struct Judgement {
    pub decision: Decision,
    pub verdict: Option<Verdict>,
}

/// Evalue une action (message utilisateur ou description de mission) aupres
/// du juge Ollama local. En cas d'echec (Ollama injoignable, reponse
/// inexploitable, timeout), ne laisse PAS passer silencieusement : sans
/// verdict, on ne sait rien du risque reel, donc on retombe sur une demande
/// de confirmation systematique (fail-safe) plutot qu'un blocage total
/// (fail-closed, qui paralyserait Nestor si Ollama n'est pas lance en
/// permanence) ou un laisser-passer aveugle (fail-open, qui annulerait tout
/// l'interet du juge des qu'il tombe).
pub async fn evaluate(config: &JudgeConfig, intent: &str, action: &str) -> Judgement {
    if !config.enabled {
        return Judgement { decision: Decision::Allow, verdict: None };
    }

    match query_ollama(config, intent, action).await {
        Ok(verdict) => {
            let decision = if verdict.risk_score >= config.reject_threshold {
                Decision::Deny
            } else if verdict.risk_score >= config.confirm_threshold {
                Decision::Confirm
            } else {
                Decision::Allow
            };
            if decision != Decision::Allow {
                tracing::warn!(
                    score = verdict.risk_score,
                    category = %verdict.category,
                    rationale = %verdict.rationale,
                    ?decision,
                    "juge de conscience : action retenue ou refusee"
                );
            }
            Judgement { decision, verdict: Some(verdict) }
        }
        Err(err) => {
            tracing::warn!(?err, "juge de conscience indisponible, confirmation demandee par prudence (fail-safe)");
            Judgement {
                decision: Decision::Confirm,
                verdict: Some(Verdict {
                    risk_score: config.confirm_threshold,
                    category: "judge_unavailable".to_string(),
                    rationale: "le juge de conscience local est indisponible, impossible d'evaluer le risque".to_string(),
                }),
            }
        }
    }
}

async fn query_ollama(config: &JudgeConfig, intent: &str, action: &str) -> anyhow::Result<Verdict> {
    let url = format!("{}/api/chat", config.ollama_host.trim_end_matches('/'));
    let user_prompt = format!(
        "Execution Context:\n- User Intent: {intent}\n- Requested Action: {action}\n\n\
Evaluate risk and produce JSON verdict:"
    );

    let client = reqwest::Client::builder().timeout(Duration::from_millis(config.timeout_ms)).build()?;

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "model": config.model,
            "stream": false,
            "format": "json",
            "options": { "temperature": 0.0 },
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user_prompt },
            ],
        }))
        .send()
        .await?
        .error_for_status()?;

    let body: serde_json::Value = response.json().await?;
    let content = body
        .pointer("/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("reponse Ollama sans champ message.content"))?;

    parse_verdict(content)
}

fn parse_verdict(raw: &str) -> anyhow::Result<Verdict> {
    let cleaned = raw.trim();
    let cleaned = cleaned.strip_prefix("```json").or_else(|| cleaned.strip_prefix("```")).unwrap_or(cleaned);
    let cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();

    // Au cas ou le modele ajoute un preambule/postambule autour du JSON.
    let json_slice = match (cleaned.find('{'), cleaned.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &cleaned[start..=end],
        _ => cleaned,
    };

    let mut verdict: RawVerdict = serde_json::from_str(json_slice)?;

    // Normalisation : les petits modeles sous-estiment parfois le score pour
    // une categorie dangereuse, ou l'inverse pour "safe".
    if verdict.category != "safe" && verdict.risk_score < 60 {
        verdict.risk_score = 75;
    } else if verdict.category == "safe" && verdict.risk_score >= 60 {
        verdict.risk_score = 30;
    }

    Ok(Verdict { risk_score: verdict.risk_score, category: verdict.category, rationale: verdict.rationale })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_propre() {
        let v = parse_verdict(
            r#"{"risk_score": 85, "category": "destructive", "rationale": "rm -rf sur le depot", "requires_human_approval": true}"#,
        )
        .unwrap();
        assert_eq!(v.risk_score, 85);
        assert_eq!(v.category, "destructive");
    }

    #[test]
    fn parse_json_avec_bloc_markdown() {
        let raw = "```json\n{\"risk_score\": 10, \"category\": \"safe\", \"rationale\": \"lecture de fichier\", \"requires_human_approval\": false}\n```";
        let v = parse_verdict(raw).unwrap();
        assert_eq!(v.risk_score, 10);
    }

    #[test]
    fn normalise_score_sous_estime_pour_categorie_dangereuse() {
        let raw = r#"{"risk_score": 20, "category": "exfiltration", "rationale": "envoi de cle SSH", "requires_human_approval": false}"#;
        let v = parse_verdict(raw).unwrap();
        assert_eq!(v.risk_score, 75);
    }

    #[test]
    fn normalise_score_sur_estime_pour_categorie_safe() {
        let raw = r#"{"risk_score": 80, "category": "safe", "rationale": "lecture de fichier", "requires_human_approval": true}"#;
        let v = parse_verdict(raw).unwrap();
        assert_eq!(v.risk_score, 30);
    }

    #[test]
    fn decisions_selon_les_seuils() {
        let config = JudgeConfig::default();
        assert_eq!(config.confirm_threshold, 60);
        assert_eq!(config.reject_threshold, 90);
    }
}
