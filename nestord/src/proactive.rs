//! Boucle proactive : la seule initiative de nestord.
//!
//! Une tache de fond se reveille a intervalle regulier, evalue des regles sur
//! l'etat du daemon (taches a relancer, missions sans progression, session en
//! mode reduit, quota qui s'epuise) et, quand l'une d'elles se declenche,
//! reinjecte une alerte dans la conversation comme rapport interne : c'est
//! Nestor qui la formule, avec sa personnalite et ses contraintes de
//! concision. Jamais de texte brut pousse au TTS.
//!
//! Discretion :
//! - dedoublonnage par evenement et par palier (`Dedup`) ;
//! - rien en heures calmes, rien quand aucun client n'est connecte (ce serait
//!   du quota depense pour personne), rien pendant que Nestor reflechit ou
//!   parle : l'alerte attend le tour suivant, et la regle est reevaluee a
//!   chaque tour, donc une condition disparue ne produit plus rien ;
//! - chaque alerte emise est publiee sur `/ws` (`ServerEvent::Alert`) et
//!   gardee pour l'instantane de connexion.
//!
//! Les regles sont pures (`evaluate`) : elles lisent une `Observation` et
//! rendent des `Alert`, ce qui les rend testables sans daemon. Les sources
//! prevues par `.agent/VISION.md` (position, agenda, mails) s'ajouteront en
//! enrichissant l'observation, pas en creant une autre boucle.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Local;
use tokio::sync::broadcast;

use crate::brain::NestorBrain;
use crate::config::{Config, ProactiveConfig};
use crate::mission::MissionManager;
use crate::protocol::{DaemonStatus, MissionStatus, ServerEvent};
use crate::todo::TodoStore;
use crate::usage::UsageState;

/// Alertes gardees pour l'instantane de connexion.
const RECENT_ALERTS: usize = 20;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static RECENT: OnceLock<Mutex<VecDeque<ServerEvent>>> = OnceLock::new();

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Dernieres alertes emises, de la plus ancienne a la plus recente.
pub fn recent_alerts() -> Vec<ServerEvent> {
    RECENT.get().map(|recent| recent.lock().unwrap().iter().cloned().collect()).unwrap_or_default()
}

fn remember(event: ServerEvent) {
    let mut recent = RECENT.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
    if recent.len() == RECENT_ALERTS {
        recent.pop_front();
    }
    recent.push_back(event);
}

/// Alerte produite par une regle, avant formulation par Nestor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Cle de dedoublonnage : une alerte par evenement et par palier.
    pub key: String,
    pub kind: &'static str,
    /// Enonce factuel, que Nestor reformule.
    pub text: String,
}

/// Ce que les regles observent, extrait de l'etat du daemon a chaque tour.
#[derive(Debug, Default)]
pub struct Observation {
    pub now_ms: u64,
    /// Missions en cours : identifiant, objet, derniere activite (epoch ms).
    pub running_missions: Vec<(u64, String, u64)>,
    /// Depuis quand la session est en mode reduit, le cas echeant.
    pub fallback_since_ms: Option<u64>,
    pub fallback_reason: Option<String>,
    /// Pire remplissage des fenetres de quota Claude, de 0 a 1.
    pub worst_utilization: Option<f32>,
    /// Lieu reconnu (`location.rs`), s'il y en a un.
    pub place: Option<String>,
    /// Rendez-vous a venir, a heure fixe (`calendar.rs`).
    pub events: Vec<crate::calendar::Event>,
}

/// Memoire des alertes deja emises, pour ne pas repeter la meme.
#[derive(Default)]
pub struct Dedup {
    emitted: HashSet<String>,
}

impl Dedup {
    /// Vrai la premiere fois que `key` est vue.
    fn first_time(&mut self, key: &str) -> bool {
        self.emitted.insert(key.to_string())
    }

    fn forget_prefix(&mut self, prefix: &str) {
        self.emitted.retain(|key| !key.starts_with(prefix));
    }

    /// L'envoi a echoue : l'alerte pourra etre retentee au tour suivant.
    fn forget(&mut self, key: &str) {
        self.emitted.remove(key);
    }
}

/// Objet d'une mission en quelques mots, pour une alerte dicable.
fn short(description: &str) -> String {
    let words: Vec<&str> = description.split_whitespace().take(8).collect();
    let mut text = words.join(" ");
    if words.len() == 8 && description.split_whitespace().count() > 8 {
        text.push('…');
    }
    text
}

/// Evalue les regles sur une observation. Pure : aucun acces au daemon.
pub fn evaluate(obs: &Observation, cfg: &ProactiveConfig, dedup: &mut Dedup) -> Vec<Alert> {
    let mut alerts = Vec::new();

    // Missions sans progression : paliers 1, 3, 6, 9… du delai configure
    // (10, 30, 60, 90 minutes avec le defaut), pour relancer sans harceler.
    let stall_ms = cfg.mission_stall_minutes.max(1) * 60_000;
    let running: HashSet<u64> = obs.running_missions.iter().map(|(id, ..)| *id).collect();
    dedup.emitted.retain(|key| match key.strip_prefix("mission:") {
        Some(rest) => rest.split(':').next().and_then(|id| id.parse().ok()).is_some_and(|id| running.contains(&id)),
        None => true,
    });
    for (id, description, last_activity_ms) in &obs.running_missions {
        let idle_ms = obs.now_ms.saturating_sub(*last_activity_ms);
        let palier = idle_ms / stall_ms;
        if palier == 0 || !(palier == 1 || palier % 3 == 0) {
            continue;
        }
        let key = format!("mission:{id}:{palier}");
        if dedup.first_time(&key) {
            alerts.push(Alert {
                key,
                kind: "mission_stalled",
                text: format!(
                    "la mission {id} ({}) n'a donne aucun signe d'activite depuis {} minutes",
                    short(description),
                    idle_ms / 60_000
                ),
            });
        }
    }

    // Session en mode reduit : la bascule elle-meme est annoncee par `brain.rs` ;
    // ici, un rappel a chaque palier tant que la situation dure.
    match obs.fallback_since_ms {
        Some(since_ms) => {
            let remind_ms = cfg.fallback_remind_minutes.max(1) * 60_000;
            let palier = obs.now_ms.saturating_sub(since_ms) / remind_ms;
            if palier >= 1 {
                let key = format!("fallback:{palier}");
                if dedup.first_time(&key) {
                    let reason = obs.fallback_reason.as_deref().unwrap_or("motif inconnu");
                    alerts.push(Alert {
                        key,
                        kind: "session_fallback",
                        text: format!(
                            "la session Claude est toujours en mode reduit, depuis {} minutes ({reason})",
                            obs.now_ms.saturating_sub(since_ms) / 60_000
                        ),
                    });
                }
            }
        }
        None => dedup.forget_prefix("fallback:"),
    }

    // Quota Claude : une alerte par tranche de cinq points au-dela du seuil.
    // On n'oublie les paliers qu'une fois nettement redescendu (hysteresis),
    // pour ne pas alerter a chaque oscillation autour du seuil.
    match obs.worst_utilization {
        Some(utilization) if utilization >= cfg.quota_threshold => {
            let step = ((utilization * 100.0).round() as u32) / 5 * 5;
            let key = format!("quota:{step}");
            if dedup.first_time(&key) {
                alerts.push(Alert {
                    key,
                    kind: "quota",
                    text: format!("le quota de la session Claude est a {} %", (utilization * 100.0).round() as u32),
                });
            }
        }
        Some(utilization) if utilization < cfg.quota_threshold - 0.1 => dedup.forget_prefix("quota:"),
        _ => {}
    }

    // Rendez-vous : partir a temps (lieu connu) ou rappel (visio). Les cles des
    // rendez-vous passes sont oubliees.
    dedup.emitted.retain(|key| match key.strip_prefix("event:") {
        Some(rest) => rest.split(':').next().and_then(|start| start.parse::<u64>().ok()).is_some_and(|start| start > obs.now_ms),
        None => true,
    });
    for event in obs.events.iter().filter(|e| !e.all_day && e.start_ms > obs.now_ms) {
        let start = crate::calendar::format_time(event.start_ms);
        let minutes_left = (event.start_ms - obs.now_ms) / 60_000;
        if event.online || event.location.is_none() {
            let reminder_ms = cfg.event_reminder_minutes.max(1) * 60_000;
            if event.start_ms - obs.now_ms <= reminder_ms {
                let key = format!("event:{}:reminder", event.start_ms);
                if dedup.first_time(&key) {
                    alerts.push(Alert {
                        key,
                        kind: "event_imminent",
                        text: format!("« {} » commence a {start}, dans {minutes_left} minutes", event.title),
                    });
                }
            }
            continue;
        }
        let travel_ms = cfg.default_travel_minutes * 60_000;
        let leave_at = event.start_ms.saturating_sub(travel_ms + cfg.departure_margin_minutes * 60_000);
        if obs.now_ms < leave_at {
            continue;
        }
        let location = event.location.as_deref().unwrap_or_default();
        let key = format!("event:{}:leave", event.start_ms);
        if dedup.first_time(&key) {
            alerts.push(Alert {
                key,
                kind: "departure",
                text: format!(
                    "il est temps de partir pour « {} » a {location} (debut a {start}, trajet estime {} minutes)",
                    event.title, cfg.default_travel_minutes
                ),
            });
        } else if obs.now_ms >= leave_at + 5 * 60_000 && obs.place.is_some() {
            // Cinq minutes plus tard, toujours dans un lieu connu : rappel ferme.
            let key = format!("event:{}:still-here", event.start_ms);
            if dedup.first_time(&key) {
                alerts.push(Alert {
                    key,
                    kind: "departure",
                    text: format!(
                        "rappel ferme : toujours a {}, alors que « {} » commence a {start} a {location}",
                        obs.place.as_deref().unwrap_or_default(),
                        event.title
                    ),
                });
            }
        }
    }

    alerts
}

fn observe(
    missions: &MissionManager,
    fallback_since_ms: Option<u64>,
    brain: &NestorBrain,
    usage: &UsageState,
    current_place: &Mutex<Option<String>>,
) -> Observation {
    Observation {
        now_ms: now_ms(),
        running_missions: missions
            .list()
            .into_iter()
            .filter(|m| matches!(m.status, MissionStatus::Started))
            .map(|m| (m.id, m.description, m.last_activity_ms))
            .collect(),
        fallback_since_ms,
        fallback_reason: brain.fallback_reason(),
        worst_utilization: usage.worst_utilization(),
        place: current_place.lock().unwrap().clone(),
        events: crate::calendar::upcoming(),
    }
}

/// Rapport interne reinjecte dans la conversation, sur le meme principe qu'un
/// compte rendu de mission : Nestor l'annonce avec ses propres mots.
fn build_report(alerts: &[Alert], address_form: &str) -> String {
    let items: Vec<&str> = alerts.iter().map(|a| a.text.as_str()).collect();
    format!(
        "[Alerte interne : {}.] Previens {address_form} en une ou deux phrases, naturellement et sans detour, \
sans lire une liste a voix haute.",
        items.join(" ; ")
    )
}

/// Rapport de relance des taches dues, pour le rappel de debut de connexion
/// (`ws.rs`) : meme forme que les alertes de la boucle.
pub fn todo_nudge_report(due: &[crate::todo::Todo], address_form: &str) -> String {
    build_report(&[Alert { key: String::new(), kind: "todo_due", text: crate::todo::describe_due(due) }], address_form)
}

/// Lance la boucle de fond. Sans effet si `[proactive] enabled = false`.
pub fn spawn(
    events_tx: broadcast::Sender<ServerEvent>,
    brain: Arc<NestorBrain>,
    config: Arc<Config>,
    todos: Arc<TodoStore>,
    missions: Arc<MissionManager>,
    usage: Arc<UsageState>,
    current_place: Arc<Mutex<Option<String>>>,
) {
    let cfg = config.proactive.clone();
    if !cfg.enabled {
        tracing::info!("boucle proactive desactivee par la configuration");
        return;
    }
    let mut events_rx = events_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(cfg.interval_secs.max(10)));
        let mut dedup = Dedup::default();
        let mut status = DaemonStatus::Idle;
        let mut fallback_since_ms: Option<u64> = None;
        let mut last_todo_check_ms: u64 = 0;
        let mut last_mail_check_ms: u64 = now_ms();
        loop {
            tokio::select! {
                event = events_rx.recv() => match event {
                    Ok(ServerEvent::State { status: next }) => status = next,
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = interval.tick() => {
                    let now = now_ms();
                    // Suivi du mode reduit a chaque tour, meme quand on se tait ensuite :
                    // c'est la duree depuis la bascule qui compte.
                    fallback_since_ms = match (brain.is_fallback(), fallback_since_ms) {
                        (true, None) => Some(now),
                        (true, since) => since,
                        (false, _) => None,
                    };

                    if config.quiet_hours.contains(Local::now().time()) {
                        continue;
                    }
                    if crate::dashboard::client_count() == 0 {
                        // Personne pour entendre : l'alerte attend la prochaine connexion.
                        continue;
                    }
                    if matches!(status, DaemonStatus::Thinking | DaemonStatus::Speaking) {
                        // On n'interrompt pas une reponse en cours : tour suivant.
                        continue;
                    }

                    // Point sur les mails : une consigne a la session (qui a Gmail par ses
                    // connecteurs), jamais une lecture a voix haute. Desactive par defaut.
                    if cfg.mail_check_minutes > 0 && now.saturating_sub(last_mail_check_ms) >= cfg.mail_check_minutes * 60_000 {
                        last_mail_check_ms = now;
                        let consigne = format!(
                            "[Consigne interne : fais le point sur les mails non lus importants de {}. \
Ne signale que ce qui merite attention, en une phrase ; s'il n'y a rien, ne dis rien.]",
                            config.address_form
                        );
                        if let Err(err) = brain.send_internal_report(&consigne).await {
                            tracing::error!(?err, "echec de la consigne de point mails");
                        }
                    }

                    let mut alerts =
                        evaluate(&observe(&missions, fallback_since_ms, &brain, &usage, &current_place), &cfg, &mut dedup);

                    // Taches a relancer : la memoire de dedoublonnage est celle du
                    // magasin (`mark_notified`), a une cadence plus lente.
                    let mut notified_todos: Vec<i64> = Vec::new();
                    if now.saturating_sub(last_todo_check_ms) >= cfg.todo_interval_minutes.max(1) * 60_000 {
                        last_todo_check_ms = now;
                        match todos.due_now() {
                            Ok(due) if !due.is_empty() => {
                                notified_todos = due.iter().map(|t| t.id).collect();
                                alerts.push(Alert { key: String::new(), kind: "todo_due", text: crate::todo::describe_due(&due) });
                            }
                            Ok(_) => {}
                            Err(err) => tracing::error!(?err, "echec de lecture des taches dues"),
                        }
                    }
                    if alerts.is_empty() {
                        continue;
                    }

                    let report = build_report(&alerts, &config.address_form);
                    tracing::info!(count = alerts.len(), kinds = ?alerts.iter().map(|a| a.kind).collect::<Vec<_>>(), "alerte proactive");
                    if let Err(err) = brain.send_internal_report(&report).await {
                        tracing::error!(?err, "echec d'envoi de l'alerte proactive");
                        for alert in &alerts {
                            dedup.forget(&alert.key);
                        }
                        continue;
                    }
                    if !notified_todos.is_empty() {
                        if let Err(err) = todos.mark_notified(&notified_todos) {
                            tracing::error!(?err, "echec de marquage des taches relancees");
                        }
                    }
                    for alert in alerts {
                        let event = ServerEvent::Alert {
                            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
                            kind: alert.kind.to_string(),
                            text: alert.text,
                            at_ms: now,
                        };
                        remember(event.clone());
                        let _ = events_tx.send(event);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn cfg() -> ProactiveConfig {
        ProactiveConfig::default()
    }

    fn keys(alerts: &[Alert]) -> Vec<&str> {
        alerts.iter().map(|a| a.key.as_str()).collect()
    }

    #[test]
    fn mission_bloquee_alerte_par_palier_puis_se_tait() {
        let mut dedup = Dedup::default();
        let mission = |idle_min: u64| Observation {
            now_ms: 1_000 * MIN,
            running_missions: vec![(7, "audit du code de nestord".to_string(), 1_000 * MIN - idle_min * MIN)],
            ..Default::default()
        };

        assert!(evaluate(&mission(5), &cfg(), &mut dedup).is_empty(), "sous le seuil : rien");
        let first = evaluate(&mission(12), &cfg(), &mut dedup);
        assert_eq!(keys(&first), ["mission:7:1"]);
        assert!(first[0].text.contains("mission 7") && first[0].text.contains("12 minutes"));
        assert!(evaluate(&mission(15), &cfg(), &mut dedup).is_empty(), "meme palier : pas de repetition");
        assert!(evaluate(&mission(25), &cfg(), &mut dedup).is_empty(), "palier 2 : silence");
        assert_eq!(keys(&evaluate(&mission(31), &cfg(), &mut dedup)), ["mission:7:3"]);
        assert_eq!(keys(&evaluate(&mission(61), &cfg(), &mut dedup)), ["mission:7:6"]);

        // Mission terminee : sa memoire est oubliee, une nouvelle mission 7 repartirait de zero.
        assert!(evaluate(&Observation { now_ms: 1_000 * MIN, ..Default::default() }, &cfg(), &mut dedup).is_empty());
        assert!(dedup.emitted.is_empty());
    }

    #[test]
    fn mode_reduit_rappele_par_palier_et_oublie_au_retour() {
        let mut dedup = Dedup::default();
        let fallback = |elapsed_min: u64| Observation {
            now_ms: 500 * MIN,
            fallback_since_ms: Some(500 * MIN - elapsed_min * MIN),
            fallback_reason: Some("Quota Claude epuise".to_string()),
            ..Default::default()
        };
        assert!(evaluate(&fallback(10), &cfg(), &mut dedup).is_empty(), "la bascule est deja annoncee par brain.rs");
        let first = evaluate(&fallback(31), &cfg(), &mut dedup);
        assert_eq!(keys(&first), ["fallback:1"]);
        assert!(first[0].text.contains("Quota Claude epuise"));
        assert!(evaluate(&fallback(45), &cfg(), &mut dedup).is_empty());
        assert_eq!(keys(&evaluate(&fallback(61), &cfg(), &mut dedup)), ["fallback:2"]);

        // Retour en mode normal, puis nouvelle bascule : on repart du premier palier.
        assert!(evaluate(&Observation { now_ms: 500 * MIN, ..Default::default() }, &cfg(), &mut dedup).is_empty());
        assert_eq!(keys(&evaluate(&fallback(31), &cfg(), &mut dedup)), ["fallback:1"]);
    }

    #[test]
    fn quota_par_tranche_avec_hysteresis() {
        let mut dedup = Dedup::default();
        let quota = |utilization: f32| Observation { now_ms: MIN, worst_utilization: Some(utilization), ..Default::default() };

        assert!(evaluate(&quota(0.80), &cfg(), &mut dedup).is_empty(), "sous le seuil");
        assert_eq!(keys(&evaluate(&quota(0.90), &cfg(), &mut dedup)), ["quota:90"]);
        assert!(evaluate(&quota(0.93), &cfg(), &mut dedup).is_empty(), "meme tranche");
        assert_eq!(keys(&evaluate(&quota(0.96), &cfg(), &mut dedup)), ["quota:95"]);
        // Oscillation juste sous le seuil : aucune repetition.
        assert!(evaluate(&quota(0.88), &cfg(), &mut dedup).is_empty());
        assert!(evaluate(&quota(0.91), &cfg(), &mut dedup).is_empty());
        // Fenetre reinitialisee : nouvelle montee, nouvelle alerte.
        assert!(evaluate(&quota(0.10), &cfg(), &mut dedup).is_empty());
        assert_eq!(keys(&evaluate(&quota(0.90), &cfg(), &mut dedup)), ["quota:90"]);
    }

    fn rdv(title: &str, start_min: u64, location: Option<&str>, online: bool) -> crate::calendar::Event {
        crate::calendar::Event {
            title: title.to_string(),
            start_ms: start_min * MIN,
            end_ms: (start_min + 60) * MIN,
            all_day: false,
            location: location.map(str::to_string),
            online,
        }
    }

    #[test]
    fn depart_a_temps_puis_rappel_ferme_si_toujours_la() {
        let mut dedup = Dedup::default();
        // Dentiste a T+1000 min, trajet 30 + marge 10 : partir a T+960.
        let obs = |now_min: u64, place: Option<&str>| Observation {
            now_ms: now_min * MIN,
            place: place.map(str::to_string),
            events: vec![rdv("Dentiste", 1000, Some("12 rue de la Paix"), false)],
            ..Default::default()
        };
        assert!(evaluate(&obs(950, Some("domicile")), &cfg(), &mut dedup).is_empty(), "trop tot");
        let leave = evaluate(&obs(961, Some("domicile")), &cfg(), &mut dedup);
        assert_eq!(keys(&leave), ["event:60000000:leave"]);
        assert!(leave[0].text.contains("Dentiste") && leave[0].text.contains("12 rue de la Paix"));
        assert!(evaluate(&obs(963, Some("domicile")), &cfg(), &mut dedup).is_empty(), "pas encore cinq minutes");
        assert!(evaluate(&obs(967, None), &cfg(), &mut dedup).is_empty(), "parti (lieu inconnu) : pas de rappel");
        let firm = evaluate(&obs(968, Some("domicile")), &cfg(), &mut dedup);
        assert_eq!(keys(&firm), ["event:60000000:still-here"]);
        assert!(firm[0].text.contains("toujours a domicile"));
        assert!(evaluate(&obs(990, Some("domicile")), &cfg(), &mut dedup).is_empty(), "une seule relance");
        // Rendez-vous passe : memoire oubliee.
        assert!(evaluate(&Observation { now_ms: 1100 * MIN, ..Default::default() }, &cfg(), &mut dedup).is_empty());
        assert!(dedup.emitted.is_empty());
    }

    #[test]
    fn visio_rappelee_dix_minutes_avant_une_seule_fois() {
        let mut dedup = Dedup::default();
        let obs = |now_min: u64| Observation {
            now_ms: now_min * MIN,
            events: vec![rdv("Point equipe", 500, None, true)],
            ..Default::default()
        };
        assert!(evaluate(&obs(480), &cfg(), &mut dedup).is_empty());
        let reminder = evaluate(&obs(491), &cfg(), &mut dedup);
        assert_eq!(keys(&reminder), ["event:30000000:reminder"]);
        assert!(reminder[0].text.contains("Point equipe") && reminder[0].text.contains("9 minutes"));
        assert!(evaluate(&obs(495), &cfg(), &mut dedup).is_empty());
    }

    #[test]
    fn rapport_regroupe_les_alertes() {
        let alerts = vec![
            Alert { key: "a".into(), kind: "quota", text: "le quota est a 90 %".into() },
            Alert { key: "b".into(), kind: "todo_due", text: "1 tache a relancer : Payer la facture".into() },
        ];
        let report = build_report(&alerts, "Monsieur");
        assert!(report.starts_with("[Alerte interne : le quota est a 90 % ; 1 tache a relancer : Payer la facture.]"));
        assert!(report.contains("Previens Monsieur"));
    }
}
