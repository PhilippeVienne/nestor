//! Serveur MCP (JSON-RPC 2.0 sur HTTP) expose a la session conversationnelle.
//!
//! Sert uniquement a lui donner de quoi deleguer : `start_mission` et
//! `list_missions`. Le transport HTTP evite un second processus - le CLI s'y
//! connecte via `--mcp-config` au demarrage de session, donc ce serveur doit
//! ecouter avant le spawn de `claude`.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use chrono::TimeZone;
use serde_json::{json, Value};

use crate::mission::Backend;
use crate::ws::AppState;

const PROTOCOL_VERSION: &str = "2025-06-18";

pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<Value>,
) -> impl IntoResponse {
    // Meme regle que `/ws` : avec un jeton configure, il est exige ici aussi, car ce
    // serveur relaie les connecteurs personnels.
    if let Some(expected) = &state.config.auth_token {
        let presented = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if presented != Some(expected.as_str()) {
            tracing::warn!("requete /mcp refusee : jeton manquant ou invalide");
            return (axum::http::StatusCode::UNAUTHORIZED, "jeton invalide").into_response();
        }
    }

    // Une notification JSON-RPC n'a pas d'`id` et n'attend pas de reponse.
    let Some(id) = request.get("id").cloned() else {
        return Json(json!({})).into_response();
    };

    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
    tracing::debug!(method, "requete MCP");

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "nestor", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools = tool_definitions().as_array().cloned().unwrap_or_default();
            if let Some(connectors) = crate::connectors::global() {
                tools.extend(connectors.tool_definitions());
            }
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => call_tool(&state, request.get("params")).await,
        other => Err(format!("methode inconnue : {other}")),
    };

    match result {
        Ok(result) => Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response(),
        Err(message) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": message },
        }))
        .into_response(),
    }
}

/// Noms des outils exposes par le serveur MCP interne (panneau « Connecteurs »).
pub fn tool_names() -> Vec<String> {
    tool_definitions()
        .as_array()
        .map(|tools| tools.iter().filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string)).collect())
        .unwrap_or_default()
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "start_mission",
            "description": "Delegue une tache longue (code, recherche, fichiers, tests) a un \
sous-agent qui travaille en arriere-plan, et retourne immediatement son identifiant. \
A utiliser des que la demande demande plus que quelques secondes de travail, pour ne pas \
bloquer la conversation vocale. Le compte rendu arrivera plus tard dans la conversation. \
Un juge de conscience local evalue la description avant lancement : une mission jugee a \
risque est refusee avec le motif - reformule ou demande confirmation explicite a \
l'utilisateur puis rappelle l'outil avec confirmed=true. Au-dela d'un certain risque, \
aucune confirmation n'est acceptee.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "La mission a accomplir, formulee de facon autonome : le \
sous-agent ne voit pas l'historique de la conversation."
                    },
                    "backend": {
                        "type": "string",
                        "enum": ["claude", "agy"],
                        "description": "Backend d'execution. Par defaut claude, avec bascule \
automatique vers agy quand le quota Claude est presque epuise."
                    },
                    "confirmed": {
                        "type": "boolean",
                        "description": "A true uniquement si l'utilisateur a explicitement \
confirme apres un refus du juge pour risque modere. Ne pas inventer une confirmation."
                    }
                },
                "required": ["description"]
            }
        },
        {
            "name": "stop_mission",
            "description": "Annule une mission en cours, identifiee par son numero. A utiliser quand \
l'utilisateur demande d'arreter ou quand la mission part dans la mauvaise direction.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Numero de la mission a annuler" },
                    "reason": {
                        "type": "string",
                        "description": "Motif de l'annulation, repris dans le compte rendu (ex: \
« l'utilisateur a change d'avis », « mission partie dans la mauvaise direction »)."
                    }
                },
                "required": ["id"]
            }
        },
        {
            "name": "list_missions",
            "description": "Liste les missions deleguees avec leur etat, leur derniere activite \
connue et leur compte rendu. A utiliser quand l'utilisateur demande ou en est une mission.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_context",
            "description": "Donne le contexte courant : lieu reconnu (domicile, bureau, etc. si \
connu depuis la position GPS du front), si on est en heures calmes, et un resume des taches en \
attente. A utiliser avant de prendre l'initiative de parler, ou quand l'utilisateur demande ou \
il est cense se trouver.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "todo_add",
            "description": "Retient une tache a faire pour l'utilisateur, ponctuelle ou recurrente. \
A utiliser des que l'utilisateur demande de se souvenir de quelque chose, mentionne une chose a \
faire, ou une habitude a prendre. Ne demande pas de confirmation superflue : retiens directement.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Intitule bref, prononcable tel quel" },
                    "notes": { "type": "string", "description": "Details optionnels" },
                    "due_at": {
                        "type": "string",
                        "description": "Echeance au format ISO 8601 (\"2026-09-20T18:00:00\"), pour \
une tache ponctuelle uniquement. Omis si pas d'echeance ou si recurrente."
                    },
                    "recurrence": {
                        "type": "string",
                        "description": "Pour une tache qui revient : \"daily\", \"weekly:<lun|mar|mer|jeu|ven|sam|dim>\", \
ou \"monthly:<1-31>\". Omis pour une tache ponctuelle."
                    }
                },
                "required": ["title"]
            }
        },
        {
            "name": "todo_list",
            "description": "Liste les taches en attente. A utiliser quand l'utilisateur demande ce \
qu'il a a faire, ou pour verifier avant d'en ajouter une similaire.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_done": { "type": "boolean", "description": "Inclure les taches deja terminees (defaut : non)" }
                }
            }
        },
        {
            "name": "todo_complete",
            "description": "Marque une tache faite. Pour une tache recurrente, cela ne fait \
que confirmer l'occurrence du jour : elle reapparaitra a la prochaine echeance de sa regle.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Identifiant de la tache" } },
                "required": ["id"]
            }
        },
        {
            "name": "todo_delete",
            "description": "Supprime definitivement une tache (utilisateur qui a change d'avis, doublon, etc.).",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Identifiant de la tache" } },
                "required": ["id"]
            }
        }
    ])
}

async fn call_tool(state: &Arc<AppState>, params: Option<&Value>) -> Result<Value, String> {
    let params = params.ok_or_else(|| "params manquants".to_string())?;
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    match name {
        "start_mission" => {
            let description = arguments
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .ok_or_else(|| "argument 'description' requis".to_string())?;

            let confirmed = arguments.get("confirmed").and_then(Value::as_bool).unwrap_or(false);

            let judge_config = crate::settings::get().judge_config(&state.config.judge);
            let judgement = crate::judge::evaluate(&judge_config, description, description).await;
            state.brain.record_verdict("mission", description, &judgement, false);
            match judgement.decision {
                crate::judge::Decision::Deny => {
                    let rationale = judgement.verdict.map(|v| v.rationale).unwrap_or_default();
                    return Ok(text_result(format!(
                        "Mission refusee par le juge de conscience, risque trop eleve : {rationale}"
                    )));
                }
                crate::judge::Decision::Confirm if !confirmed => {
                    let rationale = judgement.verdict.map(|v| v.rationale).unwrap_or_default();
                    return Ok(text_result(format!(
                        "Le juge de conscience signale un risque modere : {rationale} Demande \
confirmation explicite a l'utilisateur, puis rappelle start_mission avec confirmed=true si \
il confirme."
                    )));
                }
                _ => {}
            }

            let requested = arguments.get("backend").and_then(Value::as_str).and_then(Backend::parse);

            let record = state.missions.start(description.to_string(), requested);
            Ok(text_result(format!(
                "Mission {} lancee en arriere-plan sur {}. Le compte rendu arrivera dans la conversation.",
                record.id, record.backend
            )))
        }
        "stop_mission" => {
            let id = arguments
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "argument 'id' requis".to_string())?;

            let reason = arguments
                .get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(|r| format!("arret demande : {r}"))
                .unwrap_or_else(|| "arret demande par l'utilisateur".to_string());

            if state.missions.cancel(id, Some(reason)) {
                Ok(text_result(format!("Mission {id} annulee.")))
            } else {
                Ok(text_result(format!("Mission {id} inconnue ou deja terminee.")))
            }
        }
        "list_missions" => {
            let missions = state.missions.list();
            if missions.is_empty() {
                return Ok(text_result("Aucune mission.".to_string()));
            }
            let lines: Vec<String> = missions
                .iter()
                .map(|m| {
                    let status = match m.status {
                        crate::protocol::MissionStatus::Started => "en cours",
                        crate::protocol::MissionStatus::Completed => "terminee",
                        crate::protocol::MissionStatus::Failed => "en echec",
                        crate::protocol::MissionStatus::Cancelled => "annulee",
                    };
                    match (&m.summary, &m.progress) {
                        (Some(summary), _) => {
                            format!("#{} ({}) {status} : {} -> {summary}", m.id, m.backend, m.description)
                        }
                        (None, Some(progress)) => format!(
                            "#{} ({}) {status} : {} [en cours : {progress}]",
                            m.id, m.backend, m.description
                        ),
                        (None, None) => format!("#{} ({}) {status} : {}", m.id, m.backend, m.description),
                    }
                })
                .collect();
            Ok(text_result(lines.join("\n")))
        }
        "get_context" => {
            let place = state.current_place.lock().unwrap().clone();
            let now = chrono::Local::now();
            let is_quiet = state.config.quiet_hours.contains(now.time());

            let place_desc = place.unwrap_or_else(|| "lieu inconnu".to_string());
            let quiet_desc = if is_quiet { "en heures calmes" } else { "hors heures calmes" };
            let (pending, overdue) = state.todos.summary_counts().unwrap_or((0, 0));

            Ok(text_result(format!(
                "Lieu : {place_desc}. {quiet_desc} ({}-{}). Heure locale : {}. \
Taches en attente : {pending} (dont {overdue} en retard).",
                state.config.quiet_hours.start,
                state.config.quiet_hours.end,
                now.format("%H:%M")
            )))
        }
        "todo_add" => {
            let title = arguments
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| "argument 'title' requis".to_string())?;

            let notes = arguments.get("notes").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());

            let due_at = arguments
                .get("due_at")
                .and_then(Value::as_str)
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
                .and_then(|naive| chrono::Local.from_local_datetime(&naive).single())
                .map(|dt| dt.timestamp());

            let recurrence = arguments
                .get("recurrence")
                .and_then(Value::as_str)
                .and_then(crate::todo::Recurrence::parse);

            if arguments.get("recurrence").and_then(Value::as_str).is_some() && recurrence.is_none() {
                return Err("recurrence invalide : attendu daily, weekly:<jour> ou monthly:<1-31>".to_string());
            }

            let id = state
                .todos
                .add(title, notes, due_at, recurrence)
                .map_err(|err| format!("echec d'enregistrement : {err}"))?;

            let _ = state.events_tx.send(crate::dashboard::todos_event(&state.todos));
            Ok(text_result(format!("Tache #{id} retenue : {title}.")))
        }
        "todo_list" => {
            let include_done = arguments.get("include_done").and_then(Value::as_bool).unwrap_or(false);
            let items = state.todos.list(include_done).map_err(|err| format!("echec de lecture : {err}"))?;

            if items.is_empty() {
                return Ok(text_result("Aucune tache en attente.".to_string()));
            }

            let lines: Vec<String> = items
                .iter()
                .map(|t| {
                    let suffix = match (&t.recurrence, t.due_at) {
                        (Some(rule), _) => format!(" [recurrente : {rule}]"),
                        (None, Some(due)) => format!(" [echeance {}]", crate::todo::format_due(due)),
                        (None, None) => String::new(),
                    };
                    let done = if t.status == "done" { " (faite)" } else { "" };
                    format!("#{} {}{}{}", t.id, t.title, suffix, done)
                })
                .collect();
            Ok(text_result(lines.join("\n")))
        }
        "todo_complete" => {
            let id = arguments.get("id").and_then(Value::as_i64).ok_or_else(|| "argument 'id' requis".to_string())?;
            let found = state.todos.complete(id).map_err(|err| format!("echec de mise a jour : {err}"))?;
            let _ = state.events_tx.send(crate::dashboard::todos_event(&state.todos));
            if found {
                Ok(text_result(format!("Tache #{id} marquee faite.")))
            } else {
                Ok(text_result(format!("Tache #{id} inconnue.")))
            }
        }
        "todo_delete" => {
            let id = arguments.get("id").and_then(Value::as_i64).ok_or_else(|| "argument 'id' requis".to_string())?;
            let found = state.todos.delete(id).map_err(|err| format!("echec de suppression : {err}"))?;
            let _ = state.events_tx.send(crate::dashboard::todos_event(&state.todos));
            if found {
                Ok(text_result(format!("Tache #{id} supprimee.")))
            } else {
                Ok(text_result(format!("Tache #{id} inconnue.")))
            }
        }
        other => {
            // Outil d'un connecteur externe : relaye par la passerelle, qui applique sa regle.
            let relayed = match crate::connectors::global() {
                Some(connectors) => {
                    let brain = state.brain.clone();
                    connectors.call(other, arguments, async move |text: String| brain.announce_to_user(&text).await).await
                }
                None => None,
            };
            relayed.unwrap_or_else(|| Err(format!("outil inconnu : {other}")))
        }
    }
}

fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}
