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
use serde_json::{json, Value};

use crate::mission::Backend;
use crate::ws::AppState;

const PROTOCOL_VERSION: &str = "2025-06-18";

pub async fn mcp_handler(State(state): State<Arc<AppState>>, Json(request): Json<Value>) -> impl IntoResponse {
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
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(&state, request.get("params")),
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

fn tool_definitions() -> Value {
    json!([
        {
            "name": "start_mission",
            "description": "Delegue une tache longue (code, recherche, fichiers, tests) a un \
sous-agent qui travaille en arriere-plan, et retourne immediatement son identifiant. \
A utiliser des que la demande demande plus que quelques secondes de travail, pour ne pas \
bloquer la conversation vocale. Le compte rendu arrivera plus tard dans la conversation.",
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
connu depuis la position GPS du front), et si on est en heures calmes. A utiliser avant de \
prendre l'initiative de parler, ou quand l'utilisateur demande ou il est cense se trouver.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn call_tool(state: &Arc<AppState>, params: Option<&Value>) -> Result<Value, String> {
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

            Ok(text_result(format!(
                "Lieu : {place_desc}. {quiet_desc} ({}-{}). Heure locale : {}.",
                state.config.quiet_hours.start,
                state.config.quiet_hours.end,
                now.format("%H:%M")
            )))
        }
        other => Err(format!("outil inconnu : {other}")),
    }
}

fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}
