//! Agenda : lecture directe de Google Calendar depuis nestord (etape F de
//! `.agent/VISION.md`).
//!
//! Lecture directe plutot que via la session `claude` : la boucle proactive et,
//! plus tard, le calcul du reveil doivent connaitre le prochain rendez-vous
//! sans depenser de tour de conversation ni de quota, et meme quand la session
//! est occupee ou morte. L'ecriture (creer, deplacer) reste au connecteur de la
//! session.
//!
//! Enrolement : `nestord onboard --google` (OAuth « application de bureau »,
//! redirection sur la boucle locale, PKCE). Seul le jeton de rafraichissement est
//! conserve, dans `~/.config/nestord/google_token.json` (droits 0600). Portee
//! demandee : `calendar.events.readonly`, lecture seule.
//!
//! Le daemon relit les evenements des prochaines `horizon_hours` toutes les
//! `poll_minutes`, les garde en memoire (`upcoming`) et rediffuse le contexte
//! quand le prochain rendez-vous change.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use crate::config::{Config, GoogleConfig};
use crate::protocol::ServerEvent;
use crate::clock::now_ms;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const EVENTS_URL: &str = "https://www.googleapis.com/calendar/v3/calendars";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.readonly";

/// Rendez-vous tel que publie dans le contexte et lu par la boucle proactive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub title: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub all_day: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Visio ou lieu absent : pas de trajet a prevoir.
    pub online: bool,
}

static UPCOMING: Mutex<Vec<Event>> = Mutex::new(Vec::new());

/// Evenements a venir, du plus proche au plus lointain (les journees entieres comprises).
pub fn upcoming() -> Vec<Event> {
    UPCOMING.lock().unwrap().clone()
}

/// Prochain rendez-vous a heure fixe (une journee entiere n'est pas un rendez-vous).
pub fn next_event() -> Option<Event> {
    UPCOMING.lock().unwrap().iter().find(|e| !e.all_day).cloned()
}


pub fn token_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/nestord/google_token.json")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredToken {
    refresh_token: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    expires_at_ms: u64,
}

fn write_private(path: &PathBuf, content: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(content)?;
    Ok(())
}

fn load_token() -> Option<StoredToken> {
    let raw = std::fs::read_to_string(token_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Identifiants OAuth : `[google]` de `config.toml`, ou les variables
/// `NESTORD_GOOGLE_CLIENT_ID` / `NESTORD_GOOGLE_CLIENT_SECRET` (prioritaires).
fn credentials(cfg: &GoogleConfig) -> Option<(String, Option<String>)> {
    let id = std::env::var("NESTORD_GOOGLE_CLIENT_ID").ok().filter(|v| !v.is_empty()).or_else(|| cfg.client_id.clone())?;
    let secret =
        std::env::var("NESTORD_GOOGLE_CLIENT_SECRET").ok().filter(|v| !v.is_empty()).or_else(|| cfg.client_secret.clone());
    Some((id, secret))
}

/// L'agenda est-il branche (identifiants et jeton presents) ?
pub fn configured(cfg: &GoogleConfig) -> bool {
    credentials(cfg).is_some() && token_path().exists()
}

// ------------------------------------------------------------------ OAuth

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Defi PKCE (S256) a partir du verificateur.
fn pkce_challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn form(fields: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in fields {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
}

async fn post_token(client: &reqwest::Client, body: String) -> Result<TokenResponse> {
    let response = client
        .post(TOKEN_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .context("appel du point de jeton Google")?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Google a refuse la demande de jeton ({status}) : {text}");
    }
    serde_json::from_str(&text).context("reponse de jeton illisible")
}

/// `nestord onboard --google` : ouvre le consentement Google dans le navigateur,
/// recoit le code sur la boucle locale, echange et enregistre le jeton de
/// rafraichissement. Ne demande que la lecture des evenements.
pub async fn onboard(config: &Config) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Some((client_id, client_secret)) = credentials(&config.google) else {
        println!("Aucun identifiant OAuth Google. Dans la console Google Cloud, creez un client OAuth de type");
        println!("« Application de bureau » (API Google Calendar activee), puis renseignez `config.toml` :");
        println!("\n  [google]\n  client_id = \"….apps.googleusercontent.com\"\n  client_secret = \"…\"   # fourni avec le client de bureau\n");
        println!("ou les variables NESTORD_GOOGLE_CLIENT_ID / NESTORD_GOOGLE_CLIENT_SECRET, et relancez.");
        anyhow::bail!("identifiants OAuth absents");
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.context("ecoute sur la boucle locale")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = crate::auth::random_hex(32)?;
    let state = crate::auth::random_hex(16)?;
    let url = format!(
        "{AUTH_URL}?{}",
        form(&[
            ("client_id", &client_id),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", &pkce_challenge(&verifier)),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ])
    );

    println!("Ouvrez ce lien dans votre navigateur (lecture seule de l'agenda) :\n\n  {url}\n");
    let _ = std::process::Command::new("xdg-open").arg(&url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    println!("En attente du consentement sur http://127.0.0.1:{port} …");

    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10 * 60), listener.accept())
        .await
        .context("aucune reponse en 10 minutes")?
        .context("connexion de retour")?;
    let mut buffer = vec![0u8; 8192];
    let read = socket.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let query = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or_default();
    let params: Vec<(String, String)> = url::form_urlencoded::parse(query.trim_start_matches('/').trim_start_matches('?').as_bytes())
        .into_owned()
        .collect();
    let get = |key: &str| params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());

    let outcome = match (get("code"), get("state"), get("error")) {
        (Some(code), Some(returned), None) if returned == state => Ok(code),
        (_, _, Some(error)) => Err(anyhow::anyhow!("consentement refuse : {error}")),
        _ => Err(anyhow::anyhow!("reponse de Google invalide (etat inattendu)")),
    };
    let page = match &outcome {
        Ok(_) => "<h1>Nestor</h1><p>Agenda branch&eacute;. Vous pouvez fermer cet onglet.</p>",
        Err(_) => "<h1>Nestor</h1><p>L'enr&ocirc;lement a &eacute;chou&eacute;. Revenez au terminal.</p>",
    };
    let _ = socket
        .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{page}").as_bytes())
        .await;
    let code = outcome?;

    let mut fields = vec![
        ("code", code.as_str()),
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier.as_str()),
    ];
    if let Some(secret) = client_secret.as_deref() {
        fields.push(("client_secret", secret));
    }
    let client = reqwest::Client::new();
    let token = post_token(&client, form(&fields)).await?;
    let refresh_token = token.refresh_token.context("Google n'a pas fourni de jeton de rafraichissement")?;
    let stored = StoredToken {
        refresh_token,
        access_token: token.access_token,
        expires_at_ms: now_ms() + token.expires_in.saturating_sub(60) * 1000,
    };
    write_private(&token_path(), &serde_json::to_vec_pretty(&stored)?)?;
    println!("Agenda branche : jeton enregistre dans {}. Redemarrez nestord.", token_path().display());
    Ok(())
}

/// Jeton d'acces valide, rafraichi si besoin.
async fn access_token(client: &reqwest::Client, cfg: &GoogleConfig) -> Result<String> {
    let mut stored = load_token().context("jeton Google absent : lancez `nestord onboard --google`")?;
    if now_ms() < stored.expires_at_ms && !stored.access_token.is_empty() {
        return Ok(stored.access_token);
    }
    let (client_id, client_secret) = credentials(cfg).context("identifiants OAuth absents")?;
    let mut fields = vec![
        ("client_id", client_id.as_str()),
        ("grant_type", "refresh_token"),
        ("refresh_token", stored.refresh_token.as_str()),
    ];
    if let Some(secret) = client_secret.as_deref() {
        fields.push(("client_secret", secret));
    }
    let token = post_token(client, form(&fields)).await?;
    stored.access_token = token.access_token.clone();
    stored.expires_at_ms = now_ms() + token.expires_in.saturating_sub(60) * 1000;
    if let Some(refresh) = token.refresh_token {
        stored.refresh_token = refresh;
    }
    if let Err(err) = write_private(&token_path(), &serde_json::to_vec_pretty(&stored)?) {
        tracing::warn!(?err, "jeton Google rafraichi mais non enregistre");
    }
    Ok(token.access_token)
}

// --------------------------------------------------------------- evenements

fn parse_when(when: &Value) -> Option<(u64, bool)> {
    if let Some(date_time) = when.get("dateTime").and_then(Value::as_str) {
        let parsed = DateTime::parse_from_rfc3339(date_time).ok()?;
        return Some((parsed.timestamp_millis().max(0) as u64, false));
    }
    let date = when.get("date").and_then(Value::as_str)?;
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let local = Local.from_local_datetime(&day.and_hms_opt(0, 0, 0)?).single()?;
    Some((local.timestamp_millis().max(0) as u64, true))
}

/// Evenements d'une reponse `events.list`, filtres (annules et deja finis ecartes).
pub fn parse_events(body: &Value, now_ms: u64) -> Vec<Event> {
    let Some(items) = body.get("items").and_then(Value::as_array) else { return Vec::new() };
    let mut events: Vec<Event> = items
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) != Some("cancelled"))
        .filter_map(|item| {
            let (start_ms, all_day) = parse_when(item.get("start")?)?;
            let (end_ms, _) = item.get("end").and_then(parse_when).unwrap_or((start_ms, all_day));
            if end_ms < now_ms {
                return None;
            }
            let location = item.get("location").and_then(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string);
            let has_link = item.get("hangoutLink").and_then(Value::as_str).is_some()
                || item.pointer("/conferenceData/entryPoints").and_then(Value::as_array).is_some_and(|e| !e.is_empty());
            let online = has_link || location.as_deref().is_none_or(|l| l.starts_with("http"));
            Some(Event {
                title: item.get("summary").and_then(Value::as_str).unwrap_or("(sans titre)").to_string(),
                start_ms,
                end_ms,
                all_day,
                location,
                online,
            })
        })
        .collect();
    events.sort_by_key(|e| e.start_ms);
    events
}

async fn fetch_all(client: &reqwest::Client, cfg: &GoogleConfig) -> Result<Vec<Event>> {
    let token = access_token(client, cfg).await?;
    let now = now_ms();
    let time_min = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let time_max = (Utc::now() + chrono::Duration::hours(cfg.horizon_hours.max(1) as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut all = Vec::new();
    for calendar in cfg.calendar_ids.iter().filter(|c| !c.is_empty()) {
        let url = format!(
            "{EVENTS_URL}/{}/events?{}",
            url::form_urlencoded::byte_serialize(calendar.as_bytes()).collect::<String>(),
            form(&[
                ("timeMin", time_min.as_str()),
                ("timeMax", time_max.as_str()),
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("maxResults", "50"),
            ])
        );
        let response = client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("lecture de l'agenda {calendar}"))?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            anyhow::bail!("agenda {calendar} : {status} {}", body.pointer("/error/message").and_then(Value::as_str).unwrap_or(""));
        }
        all.extend(parse_events(&body, now));
    }
    all.sort_by_key(|e| e.start_ms);
    Ok(all)
}

static LAST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Derniere erreur de lecture, pour le diagnostic dans l'interface.
pub fn last_error() -> Option<String> {
    LAST_ERROR.get().and_then(|e| e.lock().unwrap().clone())
}

/// Lance la relecture periodique. Sans identifiants ni jeton, l'agenda reste debranche.
pub fn spawn(events_tx: broadcast::Sender<ServerEvent>, config: Arc<Config>, current_place: Arc<Mutex<Option<String>>>) {
    let cfg = config.google.clone();
    if !configured(&cfg) {
        tracing::info!("agenda Google non branche (`nestord onboard --google`)");
        return;
    }
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut interval = tokio::time::interval(Duration::from_secs(cfg.poll_minutes.max(1) * 60));
        loop {
            interval.tick().await;
            match fetch_all(&client, &cfg).await {
                Ok(events) => {
                    let previous_next = next_event();
                    let count = events.len();
                    *UPCOMING.lock().unwrap() = events;
                    *LAST_ERROR.get_or_init(|| Mutex::new(None)).lock().unwrap() = None;
                    tracing::debug!(count, "agenda relu");
                    if next_event() != previous_next {
                        let _ = events_tx.send(crate::dashboard::context_event(&config, &current_place));
                    }
                }
                Err(err) => {
                    tracing::warn!(?err, "lecture de l'agenda impossible");
                    *LAST_ERROR.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(err.to_string());
                }
            }
        }
    });
}

/// Heure locale lisible d'un instant, pour les alertes et `get_context`.
pub fn format_time(ms: u64) -> String {
    Local.timestamp_millis_opt(ms as i64).single().map(|dt| dt.format("%H:%M").to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defi_pkce_conforme_a_la_rfc_7636() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn evenements_tries_filtres_et_qualifies() {
        let body = json!({ "items": [
            { "summary": "Dentiste", "location": "12 rue de la Paix, Paris",
              "start": { "dateTime": "2026-10-09T15:00:00+02:00" }, "end": { "dateTime": "2026-10-09T16:00:00+02:00" } },
            { "summary": "Point equipe", "hangoutLink": "https://meet.google.com/abc",
              "start": { "dateTime": "2026-10-09T10:00:00+02:00" }, "end": { "dateTime": "2026-10-09T10:30:00+02:00" } },
            { "summary": "Annule", "status": "cancelled",
              "start": { "dateTime": "2026-10-09T11:00:00+02:00" }, "end": { "dateTime": "2026-10-09T12:00:00+02:00" } },
            { "summary": "Deja fini",
              "start": { "dateTime": "2026-10-08T09:00:00+02:00" }, "end": { "dateTime": "2026-10-08T10:00:00+02:00" } },
            { "summary": "Ferie", "start": { "date": "2026-10-10" }, "end": { "date": "2026-10-11" } },
            { "location": "https://zoom.us/j/1", "start": { "dateTime": "2026-10-09T18:00:00+02:00" }, "end": { "dateTime": "2026-10-09T19:00:00+02:00" } }
        ]});
        let now = DateTime::parse_from_rfc3339("2026-10-09T08:00:00+02:00").unwrap().timestamp_millis() as u64;
        let events = parse_events(&body, now);
        let titles: Vec<&str> = events.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Point equipe", "Dentiste", "(sans titre)", "Ferie"]);
        assert!(events[0].online, "une visio n'a pas de trajet");
        assert!(!events[1].online && events[1].location.is_some(), "le dentiste a un lieu");
        assert!(events[2].online, "un lieu qui est un lien est une visio");
        assert!(events[3].all_day);
        assert_eq!(events[1].end_ms - events[1].start_ms, 3_600_000);
    }
}
