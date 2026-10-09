//! Sante de l'installation : ce qui doit tourner autour de nestord pour que Nestor
//! serve (juge local, bord Tailscale, certificats, sauvegarde, session). Verifie
//! toutes les cinq minutes, publie dans l'evenement `health` et dans l'instantane
//! de connexion ; la boucle proactive signale a la voix ce qui est hors service.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::NaiveDateTime;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::clock::now_ms;
use crate::config::{Config, HealthConfig};
use crate::protocol::ServerEvent;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HealthItem {
    pub name: String,
    /// `ok`, `warn` ou `down`.
    pub status: &'static str,
    pub detail: String,
}

static ITEMS: OnceLock<Mutex<Vec<HealthItem>>> = OnceLock::new();

pub fn snapshot() -> Vec<HealthItem> {
    ITEMS.get().map(|i| i.lock().unwrap().clone()).unwrap_or_default()
}

/// Noms des elements hors service, pour la boucle proactive.
pub fn down_items() -> Vec<String> {
    snapshot().into_iter().filter(|i| i.status == "down").map(|i| format!("{} : {}", i.name, i.detail)).collect()
}

fn item(name: &str, status: &'static str, detail: impl Into<String>) -> HealthItem {
    HealthItem { name: name.to_string(), status, detail: detail.into() }
}

/// `notAfter=Jan  7 18:59:33 2027 GMT` tel que l'ecrit `openssl x509 -enddate`.
pub fn parse_not_after(line: &str) -> Option<NaiveDateTime> {
    let value = line.trim().strip_prefix("notAfter=")?.trim().trim_end_matches(" GMT");
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    NaiveDateTime::parse_from_str(&compact, "%b %d %H:%M:%S %Y").ok()
}

/// Etat d'un certificat selon les jours restants.
pub fn cert_status(days_left: i64) -> (&'static str, String) {
    if days_left < 3 {
        ("down", format!("expire dans {days_left} j"))
    } else if days_left < 14 {
        ("warn", format!("expire dans {days_left} j"))
    } else {
        ("ok", format!("{days_left} j restants"))
    }
}

/// Etat de la sauvegarde selon l'age du dernier horodatage.
pub fn backup_status(age_hours: Option<i64>) -> (&'static str, String) {
    match age_hours {
        None => ("down", "aucune sauvegarde enregistree".to_string()),
        Some(h) if h > 7 * 24 => ("down", format!("derniere il y a {} j", h / 24)),
        Some(h) if h > 2 * 24 => ("warn", format!("derniere il y a {} j", h / 24)),
        Some(h) if h >= 24 => ("ok", format!("il y a {} j", h / 24)),
        Some(h) => ("ok", format!("il y a {h} h")),
    }
}

async fn check_cert(addr: &str, name: &str) -> HealthItem {
    let label = format!("Certificat {name}");
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(format!("openssl s_client -connect {addr} -servername {name} </dev/null 2>/dev/null | openssl x509 -noout -enddate"))
            .output(),
    )
    .await;
    let Ok(Ok(out)) = out else { return item(&label, "down", "lecture impossible") };
    let text = String::from_utf8_lossy(&out.stdout);
    match parse_not_after(text.trim()) {
        Some(end) => {
            let days = (end.and_utc().timestamp() - (now_ms() / 1000) as i64) / 86_400;
            let (status, detail) = cert_status(days);
            item(&label, status, detail)
        }
        None => item(&label, "down", "certificat illisible"),
    }
}

async fn check_http(client: &reqwest::Client, label: &str, url: &str, expect_json: bool) -> HealthItem {
    match client.get(url).header("Referer", url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            if !expect_json || body.trim_start().starts_with('{') {
                item(label, "ok", "repond")
            } else {
                item(label, "down", "repond, mais pas l'API attendue")
            }
        }
        Ok(resp) => item(label, "down", format!("HTTP {}", resp.status().as_u16())),
        Err(err) => item(label, "down", if err.is_timeout() { "pas de reponse".to_string() } else { "injoignable".to_string() }),
    }
}

async fn run_checks(config: &Config, cfg: &HealthConfig) -> Vec<HealthItem> {
    let mut items = Vec::new();

    // Session conversationnelle.
    items.push(if crate::claude_process::session_alive() {
        item("Session claude", "ok", "lancee")
    } else {
        item("Session claude", "down", "absente, relance en cours")
    });

    // Juge local.
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().unwrap_or_default();
    let ollama = format!("{}/api/tags", config.judge.ollama_host.trim_end_matches('/'));
    items.push(check_http(&client, "Juge (Ollama)", &ollama, false).await);

    // Bord Tailscale : l'API par le nom public, resolue vers l'adresse Tailscale.
    if let (Some(url), Some(addr)) = (cfg.edge_url.as_deref(), cfg.edge_addr.as_deref()) {
        let edge_client = match (url::Url::parse(url), addr.parse::<std::net::SocketAddr>()) {
            (Ok(parsed), Ok(sock)) => reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .resolve(parsed.host_str().unwrap_or_default(), sock)
                .build()
                .ok(),
            _ => None,
        };
        match edge_client {
            Some(c) => items.push(check_http(&c, "Acces par le tailnet", &format!("{}/auth/status", url.trim_end_matches('/')), true).await),
            None => items.push(item("Acces par le tailnet", "down", "configuration [health] invalide")),
        }
        for name in &cfg.cert_names {
            items.push(check_cert(addr, name).await);
        }
    }

    // Sauvegarde : horodatage ecrit par deploy/backup/nestor-backup.sh.
    let age_hours = std::fs::read_to_string(crate::config::expand_home(&cfg.backup_stamp))
        .ok()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|t| ((now_ms() / 1000) as i64 - t.timestamp()) / 3600);
    let (status, detail) = backup_status(age_hours);
    items.push(item("Sauvegarde", status, detail));

    items
}

pub fn event() -> ServerEvent {
    ServerEvent::Health { items: snapshot(), checked_at_ms: now_ms() }
}

/// Lance la surveillance : premiere verification dix secondes apres le demarrage,
/// puis toutes les `interval_minutes`.
pub fn spawn(events_tx: broadcast::Sender<ServerEvent>, config: Arc<Config>) {
    let cfg = config.health.clone();
    if !cfg.enabled {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            let items = run_checks(&config, &cfg).await;
            let changed = {
                let store = ITEMS.get_or_init(|| Mutex::new(Vec::new()));
                let mut current = store.lock().unwrap();
                let changed = *current != items;
                *current = items;
                changed
            };
            if changed {
                let down: Vec<String> = down_items();
                if !down.is_empty() {
                    tracing::warn!(?down, "sante : elements hors service");
                }
            }
            let _ = events_tx.send(event());
            tokio::time::sleep(Duration::from_secs(cfg.interval_minutes.max(1) * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_d_expiration_openssl() {
        let end = parse_not_after("notAfter=Jan  7 18:59:33 2027 GMT").unwrap();
        assert_eq!(end.format("%Y-%m-%d %H:%M").to_string(), "2027-01-07 18:59");
        assert!(parse_not_after("n'importe quoi").is_none());
    }

    #[test]
    fn seuils_certificat_et_sauvegarde() {
        assert_eq!(cert_status(60).0, "ok");
        assert_eq!(cert_status(10).0, "warn");
        assert_eq!(cert_status(1).0, "down");
        assert_eq!(backup_status(None).0, "down");
        assert_eq!(backup_status(Some(5)), ("ok", "il y a 5 h".to_string()));
        assert_eq!(backup_status(Some(30)).0, "ok");
        assert_eq!(backup_status(Some(60)).0, "warn");
        assert_eq!(backup_status(Some(200)).0, "down");
    }
}
