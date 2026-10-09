//! Presence devant l'ordinateur et inhibition de la veille (etape E de
//! `.agent/VISION.md`).
//!
//! - **Presence** : nestord interroge `org.gnome.Mutter.IdleMonitor` sur le bus
//!   de session (`GetIdletime`). Au-dela de `idle_minutes` d'inactivite,
//!   l'utilisateur est considere absent. Sans GNOME ni bus de session, le
//!   module se desactive et l'utilisateur est presume present : on prefere ne
//!   jamais endormir la machine plutot que de l'endormir a tort.
//! - **Inhibition** : un seul verrou logind (`org.freedesktop.login1.Manager.Inhibit`,
//!   `sleep:idle`, mode `block`), pris des qu'il existe une raison d'etre actif
//!   et relache quand il n'en reste aucune. Le motif (`why`) est lisible dans
//!   `systemd-inhibit --list` (« mission #3 en cours ») : premier reflexe de
//!   diagnostic. Quand le motif change, le nouveau verrou est pris avant de
//!   relacher l'ancien, pour ne jamais laisser de fenetre sans protection.
//! - **Reprise** : le signal `PrepareForSleep(false)` de logind marque la
//!   sortie de veille ; le contexte est rediffuse et les raisons reevaluees.
//!   Une session `claude` morte est relancee (`claude_process::ensure_alive`).
//!
//! L'etat (presence, verrou) est publie dans l'evenement `context`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::broadcast;
use zbus::zvariant::OwnedFd;

use crate::config::{Config, PowerConfig};
use crate::mission::MissionManager;
use crate::protocol::{DaemonStatus, MissionStatus, ServerEvent};

#[zbus::proxy(
    interface = "org.gnome.Mutter.IdleMonitor",
    default_service = "org.gnome.Mutter.IdleMonitor",
    default_path = "/org/gnome/Mutter/IdleMonitor/Core"
)]
trait IdleMonitor {
    /// Temps d'inactivite en millisecondes.
    fn get_idletime(&self) -> zbus::Result<u64>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    /// Le verrou dure tant que le descripteur retourne reste ouvert.
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;

    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

/// Etat publie dans le contexte (`dashboard::context_event`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct PresenceInfo {
    /// `None` : presence non mesurable (pas de GNOME ou de bus de session).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_secs: Option<u64>,
    /// Motif du verrou de veille en cours, s'il y en a un.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inhibit: Option<String>,
    /// Derniere sortie de veille (epoch ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed_at_ms: Option<u64>,
}

static PRESENCE: Mutex<PresenceInfo> =
    Mutex::new(PresenceInfo { present: None, idle_secs: None, inhibit: None, resumed_at_ms: None });

pub fn snapshot() -> PresenceInfo {
    PRESENCE.lock().unwrap().clone()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Ce qui justifie de garder la machine eveillee.
#[derive(Debug, Default)]
pub struct ActivityInputs {
    pub running_missions: Vec<u64>,
    pub status: DaemonStatus,
    pub mobile_calls: usize,
}

/// Raisons d'etre actif, dans l'ordre ou elles apparaissent dans le motif du verrou.
pub fn active_reasons(inputs: &ActivityInputs) -> Vec<String> {
    let mut reasons: Vec<String> = inputs.running_missions.iter().map(|id| format!("mission #{id} en cours")).collect();
    match inputs.status {
        DaemonStatus::Thinking => reasons.push("Nestor reflechit".to_string()),
        DaemonStatus::Speaking => reasons.push("Nestor parle".to_string()),
        DaemonStatus::Listening | DaemonStatus::Idle => {}
    }
    if inputs.mobile_calls > 0 {
        reasons.push(match inputs.mobile_calls {
            1 => "appel mobile en cours".to_string(),
            n => format!("{n} appels mobiles en cours"),
        });
    }
    reasons
}

/// L'utilisateur est present s'il a touche la machine depuis moins de `idle_minutes`.
pub fn is_present(idle_ms: u64, idle_minutes: u64) -> bool {
    idle_ms < idle_minutes.max(1) * 60_000
}

/// Verrou logind courant : le descripteur maintient l'inhibition.
struct Lock {
    why: String,
    _fd: OwnedFd,
}

async fn take_lock(login1: &Login1ManagerProxy<'_>, why: &str) -> Option<Lock> {
    match login1.inhibit("sleep:idle", "nestord", why, "block").await {
        Ok(fd) => {
            tracing::info!(why, "veille inhibee");
            Some(Lock { why: why.to_string(), _fd: fd })
        }
        Err(err) => {
            tracing::warn!(?err, "inhibition de la veille impossible");
            None
        }
    }
}

/// Lance la surveillance. Chaque brique se desactive seule si son bus manque.
pub fn spawn(
    events_tx: broadcast::Sender<ServerEvent>,
    config: Arc<Config>,
    missions: Arc<MissionManager>,
    current_place: Arc<Mutex<Option<String>>>,
) {
    let cfg: PowerConfig = config.power.clone();
    let mut events_rx = events_tx.subscribe();
    tokio::spawn(async move {
        let idle_monitor = match zbus::Connection::session().await {
            Ok(session) => match IdleMonitorProxy::new(&session).await {
                Ok(proxy) => match proxy.get_idletime().await {
                    Ok(_) => Some(proxy),
                    Err(err) => {
                        tracing::info!(?err, "IdleMonitor de GNOME indisponible : presence presumee");
                        None
                    }
                },
                Err(err) => {
                    tracing::info!(?err, "IdleMonitor de GNOME indisponible : presence presumee");
                    None
                }
            },
            Err(err) => {
                tracing::info!(?err, "pas de bus de session : presence presumee");
                None
            }
        };
        let login1 = if cfg.inhibit {
            match zbus::Connection::system().await {
                Ok(system) => Login1ManagerProxy::new(&system).await.ok(),
                Err(err) => {
                    tracing::info!(?err, "pas de bus systeme : veille non inhibee");
                    None
                }
            }
        } else {
            tracing::info!("inhibition de la veille desactivee par la configuration");
            None
        };
        let mut sleep_signals = match &login1 {
            Some(proxy) => proxy.receive_prepare_for_sleep().await.ok(),
            None => None,
        };

        let mut lock: Option<Lock> = None;
        let mut status = DaemonStatus::Idle;
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                event = events_rx.recv() => match event {
                    Ok(ServerEvent::State { status: next }) => status = next,
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                signal = async {
                    match sleep_signals.as_mut() {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let start = signal.as_ref().and_then(|s| s.args().ok()).map(|args| *args.start()).unwrap_or(false);
                    if start {
                        tracing::info!("la machine part en veille");
                        continue;
                    }
                    tracing::info!("sortie de veille : contexte recalcule");
                    PRESENCE.lock().unwrap().resumed_at_ms = Some(now_ms());
                    // Une session claude morte pendant la veille est relancee tout de suite.
                    crate::claude_process::ensure_alive();
                }
                _ = interval.tick() => {}
            }

            // Presence.
            let mut changed = false;
            if let Some(proxy) = &idle_monitor {
                if let Ok(idle_ms) = proxy.get_idletime().await {
                    let present = is_present(idle_ms, cfg.idle_minutes);
                    let mut presence = PRESENCE.lock().unwrap();
                    changed |= presence.present != Some(present);
                    presence.present = Some(present);
                    presence.idle_secs = Some(idle_ms / 1000);
                }
            }

            // Verrou de veille.
            let inputs = ActivityInputs {
                running_missions: missions
                    .list()
                    .into_iter()
                    .filter(|m| matches!(m.status, MissionStatus::Started))
                    .map(|m| m.id)
                    .collect(),
                status,
                mobile_calls: crate::dashboard::client_count_of("mobile"),
            };
            let reasons = active_reasons(&inputs);
            if let Some(proxy) = &login1 {
                let wanted = (!reasons.is_empty()).then(|| reasons.join(" ; "));
                match (&lock, wanted) {
                    (None, None) => {}
                    (Some(current), Some(why)) if current.why == why => {}
                    (_, Some(why)) => {
                        // Nouveau verrou pris avant de lacher l'ancien : pas de fenetre sans protection.
                        if let Some(next) = take_lock(proxy, &why).await {
                            lock = Some(next);
                            changed = true;
                        }
                    }
                    (Some(_), None) => {
                        tracing::info!("plus aucune raison d'etre actif : veille autorisee");
                        lock = None;
                        changed = true;
                    }
                }
                let why = lock.as_ref().map(|l| l.why.clone());
                let mut presence = PRESENCE.lock().unwrap();
                changed |= presence.inhibit != why;
                presence.inhibit = why;
            }

            if changed {
                let _ = events_tx.send(crate::dashboard::context_event(&config, &current_place));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raisons_d_etre_actif_lisibles() {
        assert!(active_reasons(&ActivityInputs::default()).is_empty());
        let reasons = active_reasons(&ActivityInputs {
            running_missions: vec![3, 5],
            status: DaemonStatus::Speaking,
            mobile_calls: 1,
        });
        assert_eq!(reasons, ["mission #3 en cours", "mission #5 en cours", "Nestor parle", "appel mobile en cours"]);
        let only_listening = active_reasons(&ActivityInputs { status: DaemonStatus::Listening, ..Default::default() });
        assert!(only_listening.is_empty(), "ecouter sans parler ne retient pas la machine");
    }

    #[test]
    fn presence_selon_le_seuil() {
        assert!(is_present(14 * 60_000, 15));
        assert!(!is_present(15 * 60_000, 15));
        assert!(is_present(30_000, 0), "seuil nul ramene a une minute");
    }

    /// Prend reellement un verrou logind sur cette machine et le voit dans
    /// `systemd-inhibit --list`. Exige un bus systeme : `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn verrou_logind_visible_puis_relache() {
        let system = zbus::Connection::system().await.expect("bus systeme");
        let login1 = Login1ManagerProxy::new(&system).await.expect("login1");
        let lock = take_lock(&login1, "test nestord").await.expect("verrou");
        let listed = std::process::Command::new("systemd-inhibit").arg("--list").output().expect("systemd-inhibit");
        let listed = String::from_utf8_lossy(&listed.stdout).to_string();
        assert!(listed.contains("nestord") && listed.contains("test nestord"), "verrou absent : {listed}");
        drop(lock);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let listed = std::process::Command::new("systemd-inhibit").arg("--list").output().expect("systemd-inhibit");
        assert!(!String::from_utf8_lossy(&listed.stdout).contains("test nestord"));
    }
}
