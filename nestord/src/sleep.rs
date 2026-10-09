//! Veille nocturne et reveil (etape G de `.agent/VISION.md`).
//!
//! Principe : la machine ne s'endort que si *toutes* les conditions sont reunies
//! (Monsieur chez lui, plus devant l'ordinateur, aucune raison d'etre actif,
//! plage de repos), et seulement apres qu'un reveil a ete programme et
//! **verifie arme**. Tant que ce n'est pas le cas, `power.rs` tient le verrou
//! « veille differee » : la machine reste disponible pour Nestor.
//!
//! Reveil : un timer systemd utilisateur transitoire `nestor-wake` avec
//! `WakeSystem=true` (un seul a la fois), dont le service appelle `POST /wake`
//! avec un secret tire au demarrage. nestord annonce alors la journee par un
//! rapport interne (premier rendez-vous, heure de depart, taches en retard) ;
//! si aucun client n'ecoute, l'annonce attend la prochaine connexion.
//!
//! Non verifie par le code : que ce materiel se reveille bien sur un timer
//! `WakeSystem` en instance utilisateur. `docs/veille.md` donne la procedure.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::calendar::Event;
use crate::config::{Config, SleepConfig, WakeConfig};
use crate::protocol::ServerEvent;
use crate::ws::AppState;
use crate::clock::now_ms;

const TIMER_UNIT: &str = "nestor-wake";

static WAKE_SECRET: OnceLock<String> = OnceLock::new();
static PENDING_ANNOUNCEMENT: Mutex<Option<String>> = Mutex::new(None);
static STATE: Mutex<SleepInfo> = Mutex::new(SleepInfo {
    sleep_managed: false,
    sleep_allowed: false,
    sleep_blockers: Vec::new(),
    wake_at_ms: None,
    wake_armed: false,
});
static SUSPEND_REQUESTED: AtomicBool = AtomicBool::new(false);
/// Dernier reveil programme recu sur `/wake` (epoch ms).
static LAST_WAKE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn wake_header_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".config/nestord/wake_header")
}

/// Ecrit l'en-tete d'autorisation du timer dans un fichier 0600 : la ligne de
/// commande de l'unite, lisible par tout utilisateur local, ne porte que le chemin.
fn write_wake_header(secret: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = wake_header_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
    writeln!(file, "Authorization: Bearer {secret}")?;
    Ok(())
}

/// Annonce de reveil en attente, remise des qu'un client ecoute et que la session est
/// prete (appele a la connexion d'un client et a la relance de la session).
pub async fn on_session_ready(brain: &crate::brain::NestorBrain) {
    if crate::dashboard::client_count() == 0 || !crate::claude_process::session_alive() {
        return;
    }
    if let Some(announcement) = take_pending_announcement() {
        if let Err(err) = brain.send_internal_report(&announcement).await {
            tracing::error!(?err, "annonce de reveil perdue");
        }
    }
}

/// Etat publie dans le contexte.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SleepInfo {
    /// nestord gere la veille (`[sleep] enabled`).
    pub sleep_managed: bool,
    /// Toutes les conditions sont reunies et le reveil est arme.
    pub sleep_allowed: bool,
    /// Ce qui retient la machine eveillee, sinon.
    pub sleep_blockers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_at_ms: Option<u64>,
    pub wake_armed: bool,
}

pub fn snapshot() -> SleepInfo {
    STATE.lock().unwrap().clone()
}

/// Motif du verrou « veille differee » pour `power.rs`, ou `None` si la machine peut dormir.
pub fn guard_reason() -> Option<String> {
    let state = STATE.lock().unwrap();
    if !state.sleep_managed || state.sleep_allowed {
        return None;
    }
    Some(format!("veille differee ({})", state.sleep_blockers.join(", ")))
}

/// Annonce de reveil en attente d'un client, consommee a la connexion (`ws.rs`).
pub fn take_pending_announcement() -> Option<String> {
    PENDING_ANNOUNCEMENT.lock().unwrap().take()
}


// ------------------------------------------------------------------- regles

/// Ce que la decision observe.
#[derive(Debug, Default)]
pub struct SleepObservation {
    pub now_ms: u64,
    pub place: Option<String>,
    /// Age de la derniere position recue.
    pub location_age_ms: Option<u64>,
    /// `None` : presence non mesurable.
    pub present: Option<bool>,
    /// Raisons d'etre actif de `power.rs` (hors veille differee).
    pub active_reasons: Vec<String>,
    pub quiet: bool,
    pub next_event_ms: Option<u64>,
    /// Temps ecoule depuis la derniere sortie de veille ou le dernier reveil programme.
    pub resumed_age_ms: Option<u64>,
}

/// Ce qui empeche la mise en veille. Vide : toutes les conditions sont reunies.
pub fn decide(obs: &SleepObservation, cfg: &SleepConfig) -> Vec<String> {
    let mut blockers = Vec::new();
    if cfg.require_home {
        match &obs.place {
            Some(place) if place == &cfg.home_place => {}
            Some(place) => blockers.push(format!("a {place}, pas au domicile")),
            None => blockers.push("position inconnue".to_string()),
        }
        if let Some(age) = obs.location_age_ms {
            if age > cfg.location_max_age_minutes.max(1) * 60_000 {
                blockers.push(format!("position vieille de {} min", age / 60_000));
            }
        }
    }
    match obs.present {
        Some(false) => {}
        Some(true) => blockers.push("devant l'ordinateur".to_string()),
        None => blockers.push("presence non mesurable".to_string()),
    }
    if !obs.active_reasons.is_empty() {
        blockers.push(format!("actif : {}", obs.active_reasons.join(" ; ")));
    }
    if let Some(age) = obs.resumed_age_ms {
        let grace = cfg.wake_grace_minutes * 60_000;
        if age < grace {
            blockers.push(format!("reveillee il y a {} min", age / 60_000));
        }
    }
    if !obs.quiet {
        match obs.next_event_ms {
            Some(start) if start.saturating_sub(obs.now_ms) < cfg.rest_free_hours.max(1) * 3_600_000 => {
                blockers.push(format!("rendez-vous dans {} min", start.saturating_sub(obs.now_ms) / 60_000));
            }
            _ => {}
        }
    }
    blockers
}

fn next_occurrence(now: DateTime<Local>, hhmm: &str) -> Option<DateTime<Local>> {
    let time = NaiveTime::parse_from_str(hhmm.trim(), "%H:%M").ok()?;
    let today = Local.from_local_datetime(&now.date_naive().and_time(time)).single()?;
    if today > now {
        Some(today)
    } else {
        Local.from_local_datetime(&(now.date_naive() + chrono::Duration::days(1)).and_time(time)).single()
    }
}

/// Heure du reveil et son motif : premier rendez-vous a heure fixe (moins la
/// preparation et le trajet s'il y a un lieu), borne par l'heure par defaut.
pub fn plan_wake(now: DateTime<Local>, events: &[Event], wake: &WakeConfig, travel_minutes: u64) -> Option<(DateTime<Local>, String)> {
    let default = next_occurrence(now, &wake.default_time).map(|at| (at, format!("heure par defaut {}", wake.default_time)));
    let from_event = events
        .iter()
        .filter(|e| !e.all_day && e.start_ms > now.timestamp_millis().max(0) as u64)
        .filter_map(|e| {
            let start = Local.timestamp_millis_opt(e.start_ms as i64).single()?;
            let travel = if e.online || e.location.is_none() { 0 } else { travel_minutes };
            let at = start - chrono::Duration::minutes(wake.preparation_minutes as i64 + travel as i64);
            (at > now + chrono::Duration::minutes(5)).then(|| (at, format!("« {} » a {}", e.title, start.format("%H:%M"))))
        })
        .next();
    match (default, from_event) {
        (Some(d), Some(e)) => Some(if e.0 < d.0 { e } else { d }),
        (d, e) => d.or(e),
    }
}

/// Rapport interne du reveil, a formuler par Nestor.
pub fn build_announcement(now: DateTime<Local>, events: &[Event], overdue: usize, travel_minutes: u64, address: &str) -> String {
    let first = events.iter().find(|e| !e.all_day && e.start_ms > now.timestamp_millis().max(0) as u64);
    let agenda = match first {
        Some(e) => {
            let start = crate::calendar::format_time(e.start_ms);
            match (&e.location, e.online) {
                (Some(location), false) => {
                    let leave = crate::calendar::format_time(e.start_ms.saturating_sub(travel_minutes * 60_000));
                    format!("Premier rendez-vous : « {} » a {start} a {location}, depart conseille vers {leave}.", e.title)
                }
                _ => format!("Premier rendez-vous : « {} » a {start} (en ligne ou sans lieu).", e.title),
            }
        }
        None => "Aucun rendez-vous a heure fixe aujourd'hui.".to_string(),
    };
    let tasks = match overdue {
        0 => String::new(),
        1 => " Une tache en retard.".to_string(),
        n => format!(" {n} taches en retard."),
    };
    format!(
        "[Reveil : il est {} ce {}. {agenda}{tasks}] Reveille {address} et annonce la journee en une ou deux phrases, sans lire une liste.",
        now.format("%H:%M"),
        french_date(now)
    )
}

/// « vendredi 10 octobre » : chrono ne connait que l'anglais.
fn french_date(now: DateTime<Local>) -> String {
    const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
    const MONTHS: [&str; 12] = [
        "janvier", "fevrier", "mars", "avril", "mai", "juin", "juillet", "aout", "septembre", "octobre", "novembre", "decembre",
    ];
    format!(
        "{} {} {}",
        DAYS[now.weekday().num_days_from_monday() as usize],
        now.day(),
        MONTHS[now.month0() as usize]
    )
}

// ------------------------------------------------------------- timer systemd

fn systemd_run_args(at: DateTime<Local>, header_path: &std::path::Path) -> Vec<String> {
    vec![
        "--user".into(),
        format!("--on-calendar={}", at.format("%Y-%m-%d %H:%M:%S")),
        "--timer-property=WakeSystem=true".into(),
        format!("--unit={TIMER_UNIT}"),
        "--collect".into(),
        "--".into(),
        "curl".into(),
        "-fsS".into(),
        "-X".into(),
        "POST".into(),
        "-H".into(),
        format!("@{}", header_path.display()),
        format!("http://{}/wake", crate::LISTEN_ADDR),
    ]
}

/// Echeance du timer `nestor-wake` (epoch ms), s'il est arme.
pub async fn armed_at() -> Option<u64> {
    let out = tokio::process::Command::new("systemctl")
        .args(["--user", "list-timers", &format!("{TIMER_UNIT}.timer"), "--output=json"])
        .output()
        .await
        .ok()?;
    parse_list_timers(&String::from_utf8_lossy(&out.stdout))
}

/// `next` de `systemctl list-timers --output=json` est en microsecondes depuis l'epoque.
fn parse_list_timers(json: &str) -> Option<u64> {
    let value: Value = serde_json::from_str(json).ok()?;
    value
        .as_array()?
        .iter()
        .find(|t| t.get("unit").and_then(Value::as_str) == Some(&format!("{TIMER_UNIT}.timer")))
        .and_then(|t| t.get("next").and_then(Value::as_u64))
        .filter(|next| *next > 0)
        .map(|next| next / 1000)
}

async fn disarm() {
    let _ = tokio::process::Command::new("systemctl")
        .args(["--user", "stop", &format!("{TIMER_UNIT}.timer")])
        .output()
        .await;
}

/// Programme le reveil (remplace le precedent) et retourne l'echeance verifiee.
async fn arm(at: DateTime<Local>, header_path: &std::path::Path) -> Result<u64> {
    disarm().await;
    let out = tokio::process::Command::new("systemd-run")
        .args(systemd_run_args(at, header_path))
        .output()
        .await
        .context("lancement de systemd-run")?;
    if !out.status.success() {
        anyhow::bail!("systemd-run a echoue : {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    // Verification independante : sans timer arme, on ne laisse pas dormir.
    armed_at().await.context("timer nestor-wake absent apres armement")
}

// ------------------------------------------------------------------- boucle

/// Lance la gestion de la veille. Sans effet si `[sleep] enabled = false`.
pub fn spawn(
    events_tx: broadcast::Sender<ServerEvent>,
    config: Arc<Config>,
    current_place: Arc<Mutex<Option<String>>>,
) {
    let cfg = config.sleep.clone();
    if !cfg.enabled {
        tracing::info!("veille nocturne non geree par nestord (configuration)");
        return;
    }
    let Ok(secret) = crate::auth::random_hex(16) else {
        tracing::error!("alea indisponible : veille nocturne non geree");
        return;
    };
    if let Err(err) = write_wake_header(&secret) {
        tracing::error!(?err, "en-tete de reveil non ecrit : veille nocturne non geree");
        return;
    }
    let _ = WAKE_SECRET.set(secret);
    let header_path = wake_header_path();
    STATE.lock().unwrap().sleep_managed = true;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let now = Local::now();
            let presence = crate::power::snapshot();
            let events = crate::calendar::upcoming();
            let obs = SleepObservation {
                now_ms: now_ms(),
                place: current_place.lock().unwrap().clone(),
                location_age_ms: crate::location::last_fix_age_ms(),
                present: presence.present,
                active_reasons: crate::power::current_reasons(),
                quiet: config.quiet_hours.contains(now.time()),
                next_event_ms: crate::calendar::next_event().map(|e| e.start_ms),
                resumed_age_ms: [presence.resumed_at_ms, Some(LAST_WAKE_MS.load(Ordering::Relaxed)).filter(|w| *w > 0)]
                    .into_iter()
                    .flatten()
                    .max()
                    .map(|at| now_ms().saturating_sub(at)),
            };
            let blockers = decide(&obs, &cfg);
            let plan = plan_wake(now, &events, &config.wake, config.proactive.default_travel_minutes);

            // Reveil arme des que la veille est possible, et rearme si l'heure change. Le
            // timer n'est relu que lorsqu'il compte : veille possible, ou timer deja arme.
            let previously_armed = STATE.lock().unwrap().wake_armed;
            let mut armed_ms = if blockers.is_empty() || previously_armed { armed_at().await } else { None };
            if blockers.is_empty() {
                if let Some((at, why)) = &plan {
                    let wanted_ms = at.timestamp_millis().max(0) as u64;
                    let differs = armed_ms.is_none_or(|a| a.abs_diff(wanted_ms) > 60_000);
                    if differs {
                        match arm(*at, &header_path).await {
                            Ok(verified) => {
                                armed_ms = Some(verified);
                                tracing::info!(reveil = %at.format("%d/%m %H:%M"), %why, "reveil programme");
                            }
                            Err(err) => {
                                armed_ms = None;
                                tracing::error!(?err, "reveil non programme : la machine reste eveillee");
                            }
                        }
                    }
                }
            }

            let next = SleepInfo {
                sleep_managed: true,
                sleep_allowed: blockers.is_empty() && armed_ms.is_some(),
                sleep_blockers: if blockers.is_empty() && armed_ms.is_none() {
                    vec!["reveil non programme".to_string()]
                } else {
                    blockers
                },
                wake_at_ms: armed_ms,
                wake_armed: armed_ms.is_some(),
            };
            let changed = {
                let mut state = STATE.lock().unwrap();
                let changed = *state != next;
                *state = next.clone();
                changed
            };
            if changed {
                tracing::info!(allowed = next.sleep_allowed, blockers = ?next.sleep_blockers, "veille reevaluee");
                let _ = events_tx.send(crate::dashboard::context_event(&config, &current_place));
            }
            if next.sleep_allowed && cfg.force_suspend && !SUSPEND_REQUESTED.swap(true, Ordering::SeqCst) {
                tracing::info!("mise en veille demandee (force_suspend)");
                let _ = tokio::process::Command::new("systemctl").arg("suspend").output().await;
            }
            if !next.sleep_allowed {
                SUSPEND_REQUESTED.store(false, Ordering::SeqCst);
            }
        }
    });
}

// -------------------------------------------------------------- POST /wake

/// Appele par le service du timer : reveille Monsieur avec l'annonce de la journee.
pub async fn wake_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    let expected = WAKE_SECRET.get().map(String::as_str).unwrap_or_default();
    if expected.is_empty() || !crate::auth::constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "secret de reveil invalide" }))).into_response();
    }
    let now = Local::now();
    LAST_WAKE_MS.store(now_ms(), Ordering::Relaxed);
    let overdue = state.todos.summary_counts().map(|(_, overdue)| overdue).unwrap_or(0);
    let report = build_announcement(now, &crate::calendar::upcoming(), overdue, state.config.proactive.default_travel_minutes, &state.config.address_form);
    tracing::info!("reveil : annonce de la journee");
    let _ = state.events_tx.send(ServerEvent::Alert {
        id: crate::proactive::next_alert_id(),
        kind: "wake".to_string(),
        text: format!("reveil programme a {}", now.format("%H:%M")),
        at_ms: now_ms(),
    });
    // Session morte pendant la nuit : relancee, et l'annonce attend qu'elle soit prete
    // (`on_session_ready`) plutot que de partir en mode reduit avec son annonce de quota.
    crate::claude_process::ensure_alive();
    let delivered = if crate::dashboard::client_count() > 0 && crate::claude_process::session_alive() {
        state.brain.send_internal_report(&report).await.is_ok()
    } else {
        false
    };
    if !delivered {
        // Personne pour l'entendre, ou session en cours de relance : l'annonce attend.
        *PENDING_ANNOUNCEMENT.lock().unwrap() = Some(report);
    }
    Json(json!({ "delivered": delivered })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SleepConfig {
        SleepConfig::default()
    }

    fn obs_ok() -> SleepObservation {
        SleepObservation {
            now_ms: 1_000_000,
            place: Some("domicile".to_string()),
            location_age_ms: Some(10 * 60_000),
            present: Some(false),
            active_reasons: Vec::new(),
            quiet: true,
            next_event_ms: None,
            resumed_age_ms: None,
        }
    }

    #[test]
    fn toutes_les_conditions_reunies_ou_chaque_blocage_nomme() {
        assert!(decide(&obs_ok(), &cfg()).is_empty());

        let away = SleepObservation { place: Some("bureau".to_string()), ..obs_ok() };
        assert_eq!(decide(&away, &cfg()), ["a bureau, pas au domicile"]);
        let unknown = SleepObservation { place: None, ..obs_ok() };
        assert_eq!(decide(&unknown, &cfg()), ["position inconnue"]);
        let stale = SleepObservation { location_age_ms: Some(5 * 3_600_000), ..obs_ok() };
        assert_eq!(decide(&stale, &cfg()), ["position vieille de 300 min"]);
        let at_desk = SleepObservation { present: Some(true), ..obs_ok() };
        assert_eq!(decide(&at_desk, &cfg()), ["devant l'ordinateur"]);
        let no_gnome = SleepObservation { present: None, ..obs_ok() };
        assert_eq!(decide(&no_gnome, &cfg()), ["presence non mesurable"]);
        let busy = SleepObservation { active_reasons: vec!["mission #2 en cours".into()], ..obs_ok() };
        assert_eq!(decide(&busy, &cfg()), ["actif : mission #2 en cours"]);

        // Hors heures calmes : repos seulement si aucun rendez-vous proche.
        let day_free = SleepObservation { quiet: false, next_event_ms: Some(1_000_000 + 10 * 3_600_000), ..obs_ok() };
        assert!(decide(&day_free, &cfg()).is_empty());
        let day_soon = SleepObservation { quiet: false, next_event_ms: Some(1_000_000 + 2 * 3_600_000), ..obs_ok() };
        assert_eq!(decide(&day_soon, &cfg()), ["rendez-vous dans 120 min"]);

        // Juste reveillee : sursis avant de redormir.
        let just_woke = SleepObservation { resumed_age_ms: Some(5 * 60_000), ..obs_ok() };
        assert_eq!(decide(&just_woke, &cfg()), ["reveillee il y a 5 min"]);
        let long_ago = SleepObservation { resumed_age_ms: Some(45 * 60_000), ..obs_ok() };
        assert!(decide(&long_ago, &cfg()).is_empty());

        // Sans exigence de domicile, la position ne compte plus.
        let relaxed = SleepConfig { require_home: false, ..cfg() };
        assert!(decide(&unknown, &relaxed).is_empty());
    }

    fn event(title: &str, start: DateTime<Local>, location: Option<&str>) -> Event {
        Event {
            title: title.into(),
            start_ms: start.timestamp_millis() as u64,
            end_ms: start.timestamp_millis() as u64 + 3_600_000,
            all_day: false,
            location: location.map(str::to_string),
            online: location.is_none(),
        }
    }

    #[test]
    fn reveil_par_defaut_ou_avance_par_le_premier_rendez_vous() {
        let now = Local.with_ymd_and_hms(2026, 10, 9, 23, 0, 0).unwrap();
        let wake = WakeConfig { default_time: "07:30".into(), preparation_minutes: 45 };

        let (at, why) = plan_wake(now, &[], &wake, 30).unwrap();
        assert_eq!(at, Local.with_ymd_and_hms(2026, 10, 10, 7, 30, 0).unwrap());
        assert!(why.contains("07:30"));

        // Dentiste a 8h00 avec lieu : 8h00 - 45 - 30 = 6h45.
        let dentist = event("Dentiste", Local.with_ymd_and_hms(2026, 10, 10, 8, 0, 0).unwrap(), Some("rue de la Paix"));
        let (at, why) = plan_wake(now, &[dentist], &wake, 30).unwrap();
        assert_eq!(at, Local.with_ymd_and_hms(2026, 10, 10, 6, 45, 0).unwrap());
        assert!(why.contains("Dentiste"));

        // Visio a 14h : le defaut reste plus tot.
        let visio = event("Point", Local.with_ymd_and_hms(2026, 10, 10, 14, 0, 0).unwrap(), None);
        let (at, _) = plan_wake(now, &[visio], &wake, 30).unwrap();
        assert_eq!(at, Local.with_ymd_and_hms(2026, 10, 10, 7, 30, 0).unwrap());

        // Rendez-vous trop proche pour un reveil utile : ignore.
        let soon = event("Nuit", Local.with_ymd_and_hms(2026, 10, 9, 23, 30, 0).unwrap(), Some("ici"));
        let (at, _) = plan_wake(now, &[soon], &wake, 30).unwrap();
        assert_eq!(at, Local.with_ymd_and_hms(2026, 10, 10, 7, 30, 0).unwrap());
    }

    #[test]
    fn annonce_de_la_journee() {
        let now = Local.with_ymd_and_hms(2026, 10, 10, 7, 30, 0).unwrap();
        let dentist = event("Dentiste", Local.with_ymd_and_hms(2026, 10, 10, 9, 0, 0).unwrap(), Some("rue de la Paix"));
        let report = build_announcement(now, &[dentist], 2, 30, "Monsieur");
        assert!(report.starts_with("[Reveil : il est 07:30 ce samedi 10 octobre."), "{report}");
        assert!(report.contains("« Dentiste » a 09:00 a rue de la Paix, depart conseille vers 08:30"));
        assert!(report.contains("2 taches en retard"));
        assert!(report.contains("Reveille Monsieur"));
        let empty = build_announcement(now, &[], 0, 30, "Madame");
        assert!(empty.contains("Aucun rendez-vous") && !empty.contains("retard"));
    }

    #[test]
    fn timer_lu_depuis_le_json_de_systemctl() {
        let json = r#"[{"next":1791545325399611,"left":1,"last":0,"passed":0,"unit":"nestor-wake.timer","activates":"nestor-wake.service"}]"#;
        assert_eq!(parse_list_timers(json), Some(1_791_545_325_399));
        assert_eq!(parse_list_timers("[]"), None);
        assert_eq!(parse_list_timers("pas du json"), None);
    }

    #[test]
    fn arguments_systemd_run() {
        let at = Local.with_ymd_and_hms(2026, 10, 10, 6, 45, 0).unwrap();
        let args = systemd_run_args(at, std::path::Path::new("/home/x/.config/nestord/wake_header"));
        assert_eq!(args[1], "--on-calendar=2026-10-10 06:45:00");
        assert!(args.contains(&"@/home/x/.config/nestord/wake_header".to_string()), "le secret ne passe pas par la ligne de commande");
        assert!(!args.iter().any(|a| a.contains("Bearer")));
        assert!(args.contains(&"--timer-property=WakeSystem=true".to_string()));
        assert!(args.contains(&"--unit=nestor-wake".to_string()));
        assert!(args.last().unwrap().ends_with("/wake"));
    }

    /// Arme reellement un timer utilisateur deux minutes plus tard, le lit puis le retire.
    #[tokio::test]
    #[ignore]
    async fn timer_reel_arme_puis_retire() {
        let at = Local::now() + chrono::Duration::minutes(2);
        let header = std::env::temp_dir().join("nestord-wake-header-test");
        std::fs::write(&header, "Authorization: Bearer test\n").unwrap();
        let verified = arm(at, &header).await.expect("armement");
        assert!(verified.abs_diff(at.timestamp_millis() as u64) < 2_000);
        disarm().await;
        assert!(armed_at().await.is_none());
    }
}
