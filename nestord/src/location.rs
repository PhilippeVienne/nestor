//! Position de l'utilisateur, recue de l'appli mobile hors appel (`POST /location`)
//! ou d'un client connecte a `/ws` (`ClientEvent::Location`).
//!
//! nestord n'en garde que le lieu reconnu (`Config::place_at`) : la position
//! elle-meme n'est ni conservee ni journalisee, c'est une donnee sensible. Le
//! lieu alimente le contexte (« Monsieur est chez lui ») et, plus tard, la
//! boucle proactive et la mise en veille.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::protocol::ServerEvent;
use crate::ws::AppState;

/// Corps de `POST /location`. `accuracy` (m) et `timestamp` (epoch ms) sont
/// acceptes pour la compatibilite avec `expo-location`, mais pas exploites.
#[derive(Debug, Deserialize)]
pub struct LocationReport {
    pub lat: f64,
    pub lon: f64,
    #[allow(dead_code)]
    pub accuracy: Option<f64>,
    #[allow(dead_code)]
    pub timestamp: Option<u64>,
    /// Jeton d'acces, a defaut d'en-tete `Authorization: Bearer`.
    pub token: Option<String>,
}

/// Met a jour le lieu courant et, s'il change, rediffuse le contexte.
/// Retourne le lieu reconnu.
pub fn apply(
    config: &Config,
    current_place: &Mutex<Option<String>>,
    events_tx: &broadcast::Sender<ServerEvent>,
    lat: f64,
    lon: f64,
) -> Option<String> {
    let place = config.place_at(lat, lon).map(str::to_string);
    let changed = {
        let mut current = current_place.lock().unwrap();
        if *current != place {
            *current = place.clone();
            true
        } else {
            false
        }
    };
    if changed {
        tracing::info!(?place, "changement de lieu detecte");
        let _ = events_tx.send(crate::dashboard::context_event(config, current_place));
    }
    place
}

/// Jeton presente : en-tete `Authorization: Bearer …`, sinon champ `token` du corps.
fn presented_token<'a>(headers: &'a HeaderMap, body: &'a LocationReport) -> &'a str {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .or(body.token.as_deref())
        .unwrap_or_default()
}

/// `POST /location` : memes preuves d'acces que `/ws` (jeton de `nestord onboard`
/// ou session par passkey). Repond `{ "place": "domicile" }` ou `{ "place": null }`.
pub async fn location_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LocationReport>,
) -> Response {
    let origin = headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok());
    if !crate::auth::origin_allowed(origin, &state.config.allowed_origins) {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "origine non autorisee" }))).into_response();
    }
    if state.config.auth.is_some() || crate::passkey::has_any() {
        let presented = presented_token(&headers, &body);
        let by_token = state.config.auth.as_ref().is_some_and(|expected| expected.matches(presented));
        if !by_token && !crate::passkey::session_valid(presented) {
            tracing::warn!("position refusee : jeton ou session invalide");
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "authentification requise" }))).into_response();
        }
    }
    if !(-90.0..=90.0).contains(&body.lat) || !(-180.0..=180.0).contains(&body.lon) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "coordonnees invalides" }))).into_response();
    }
    let place = apply(&state.config, &state.current_place, &state.events_tx, body.lat, body.lon);
    Json(json!({ "place": place })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Place;

    fn config_with_home() -> Config {
        Config {
            places: vec![Place { name: "domicile".to_string(), lat: 48.8566, lon: 2.3522, radius_m: 300.0 }],
            ..Config::default()
        }
    }

    #[test]
    fn lieu_reconnu_et_contexte_rediffuse_seulement_au_changement() {
        let config = config_with_home();
        let current = Mutex::new(None);
        let (tx, mut rx) = broadcast::channel(8);

        assert_eq!(apply(&config, &current, &tx, 48.8567, 2.3523).as_deref(), Some("domicile"));
        assert!(matches!(rx.try_recv(), Ok(ServerEvent::Context { place: Some(p), .. }) if p == "domicile"));

        // Meme lieu : rien de nouveau a diffuser.
        assert_eq!(apply(&config, &current, &tx, 48.8565, 2.3521).as_deref(), Some("domicile"));
        assert!(rx.try_recv().is_err());

        // Loin du domicile : lieu inconnu, contexte rediffuse.
        assert_eq!(apply(&config, &current, &tx, 45.76, 4.83), None);
        assert!(matches!(rx.try_recv(), Ok(ServerEvent::Context { place: None, .. })));
    }

    #[test]
    fn jeton_en_tete_prioritaire_sur_le_corps() {
        let body = LocationReport { lat: 0.0, lon: 0.0, accuracy: None, timestamp: None, token: Some("corps".into()) };
        let mut headers = HeaderMap::new();
        assert_eq!(presented_token(&headers, &body), "corps");
        headers.insert(axum::http::header::AUTHORIZATION, "Bearer en-tete".parse().unwrap());
        assert_eq!(presented_token(&headers, &body), "en-tete");
        let sans = LocationReport { lat: 0.0, lon: 0.0, accuracy: None, timestamp: None, token: None };
        assert_eq!(presented_token(&HeaderMap::new(), &sans), "");
    }
}
