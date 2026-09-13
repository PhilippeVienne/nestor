//! Memoire de taches de Nestor : ponctuelles ou recurrentes, persistees en
//! SQLite (fichier unique, aucun service). Premiere brique du modele de
//! memoire propose dans `docs/memory.md` - pas le graphe complet, seulement
//! ce qu'il faut pour que Nestor retienne ce qui reste a faire et relance.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, TimeZone};
use rusqlite::Connection;
use serde::Serialize;

/// Ne relance pas le meme rappel avant ce delai, que ce soit la nudge de
/// debut de conversation ou la boucle proactive.
const RENOTIFY_COOLDOWN_SECONDS: i64 = 6 * 3600;

fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/nestord/todos.db")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Done,
}

impl Status {
    fn parse(s: &str) -> Self {
        if s == "done" { Status::Done } else { Status::Pending }
    }
}

/// Regle de recurrence, au format texte stocke tel quel en base :
/// `daily`, `weekly:<lun|mar|mer|jeu|ven|sam|dim>`, `monthly:<1-31>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recurrence {
    Daily,
    Weekly(chrono::Weekday),
    Monthly(u32),
}

impl Recurrence {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase();
        if raw == "daily" {
            return Some(Recurrence::Daily);
        }
        if let Some(day) = raw.strip_prefix("weekly:") {
            let weekday = match day {
                "lun" | "mon" => chrono::Weekday::Mon,
                "mar" | "tue" => chrono::Weekday::Tue,
                "mer" | "wed" => chrono::Weekday::Wed,
                "jeu" | "thu" => chrono::Weekday::Thu,
                "ven" | "fri" => chrono::Weekday::Fri,
                "sam" | "sat" => chrono::Weekday::Sat,
                "dim" | "sun" => chrono::Weekday::Sun,
                _ => return None,
            };
            return Some(Recurrence::Weekly(weekday));
        }
        if let Some(day) = raw.strip_prefix("monthly:") {
            let day: u32 = day.parse().ok()?;
            if (1..=31).contains(&day) {
                return Some(Recurrence::Monthly(day));
            }
        }
        None
    }

    fn to_stored(&self) -> String {
        match self {
            Recurrence::Daily => "daily".to_string(),
            Recurrence::Weekly(d) => format!("weekly:{}", weekday_code(*d)),
            Recurrence::Monthly(d) => format!("monthly:{d}"),
        }
    }

    fn matches(&self, today: DateTime<Local>) -> bool {
        match self {
            Recurrence::Daily => true,
            Recurrence::Weekly(d) => today.weekday() == *d,
            Recurrence::Monthly(d) => today.day() == *d,
        }
    }
}

fn weekday_code(d: chrono::Weekday) -> &'static str {
    match d {
        chrono::Weekday::Mon => "lun",
        chrono::Weekday::Tue => "mar",
        chrono::Weekday::Wed => "mer",
        chrono::Weekday::Thu => "jeu",
        chrono::Weekday::Fri => "ven",
        chrono::Weekday::Sat => "sam",
        chrono::Weekday::Sun => "dim",
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub notes: Option<String>,
    pub status: String,
    pub recurrence: Option<String>,
    /// Echeance des taches ponctuelles (epoch secondes). Ignore pour les
    /// taches recurrentes.
    pub due_at: Option<i64>,
    pub created_at: i64,
}

pub struct TodoStore {
    conn: Mutex<Connection>,
}

impl TodoStore {
    pub fn open_default() -> Result<Self> {
        Self::open(&default_db_path())
    }

    fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("creation du dossier de la base de taches")?;
        }
        let conn = Connection::open(path).context("ouverture de la base de taches")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                notes TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                recurrence TEXT,
                due_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                completed_at INTEGER,
                last_notified_at INTEGER
            );",
        )
        .context("initialisation du schema de la base de taches")?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn add(
        &self,
        title: &str,
        notes: Option<&str>,
        due_at: Option<i64>,
        recurrence: Option<Recurrence>,
    ) -> Result<i64> {
        let now = Local::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO todos (title, notes, status, recurrence, due_at, created_at, updated_at)
             VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?5)",
            rusqlite::params![title, notes, recurrence.map(|r| r.to_stored()), due_at, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list(&self, include_done: bool) -> Result<Vec<Todo>> {
        let conn = self.conn.lock().unwrap();
        let sql = if include_done {
            "SELECT id, title, notes, status, recurrence, due_at, created_at FROM todos ORDER BY created_at DESC"
        } else {
            "SELECT id, title, notes, status, recurrence, due_at, created_at FROM todos \
             WHERE status != 'done' ORDER BY due_at IS NULL, due_at ASC, created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(Todo {
                id: row.get(0)?,
                title: row.get(1)?,
                notes: row.get(2)?,
                status: row.get(3)?,
                recurrence: row.get(4)?,
                due_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Marque une tache faite. Pour une tache recurrente, cela ne fait que
    /// dedoublonner l'occurrence du jour (`last_notified_at`) : elle
    /// reapparaitra a la prochaine occurrence de sa regle.
    pub fn complete(&self, id: i64) -> Result<bool> {
        let now = Local::now().timestamp();
        let conn = self.conn.lock().unwrap();
        let recurrence: Option<Option<String>> = conn
            .query_row("SELECT recurrence FROM todos WHERE id = ?1", [id], |row| row.get(0))
            .ok();

        let Some(recurrence) = recurrence else { return Ok(false) };

        let changed = if recurrence.is_some() {
            conn.execute(
                "UPDATE todos SET last_notified_at = ?2, updated_at = ?2 WHERE id = ?1",
                rusqlite::params![id, now],
            )?
        } else {
            conn.execute(
                "UPDATE todos SET status = 'done', completed_at = ?2, updated_at = ?2 WHERE id = ?1",
                rusqlite::params![id, now],
            )?
        };
        Ok(changed > 0)
    }

    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM todos WHERE id = ?1", [id])?;
        Ok(changed > 0)
    }

    /// Taches a relancer maintenant : ponctuelles en retard, ou recurrentes
    /// dont la regle correspond a aujourd'hui, dans les deux cas pas deja
    /// relancees il y a moins de [`RENOTIFY_COOLDOWN_SECONDS`].
    pub fn due_now(&self) -> Result<Vec<Todo>> {
        let now = Local::now();
        let now_ts = now.timestamp();
        let all = self.list(false)?;

        let due: Vec<Todo> = all
            .into_iter()
            .filter(|todo| {
                if Status::parse(&todo.status) == Status::Done {
                    return false;
                }

                let conn = self.conn.lock().unwrap();
                let last_notified_at: Option<i64> = conn
                    .query_row("SELECT last_notified_at FROM todos WHERE id = ?1", [todo.id], |row| row.get(0))
                    .unwrap_or(None);
                drop(conn);

                let cooldown_ok = last_notified_at
                    .map(|ts| now_ts - ts > RENOTIFY_COOLDOWN_SECONDS)
                    .unwrap_or(true);
                if !cooldown_ok {
                    return false;
                }

                if let Some(rule) = todo.recurrence.as_deref().and_then(Recurrence::parse) {
                    rule.matches(now)
                } else {
                    todo.due_at.map(|due| due <= now_ts).unwrap_or(false)
                }
            })
            .collect();

        Ok(due)
    }

    pub fn mark_notified(&self, ids: &[i64]) -> Result<()> {
        let now = Local::now().timestamp();
        let conn = self.conn.lock().unwrap();
        for id in ids {
            conn.execute("UPDATE todos SET last_notified_at = ?2 WHERE id = ?1", rusqlite::params![id, now])?;
        }
        Ok(())
    }

    /// Compte pour `get_context` : total en attente et combien sont en retard.
    pub fn summary_counts(&self) -> Result<(usize, usize)> {
        let now_ts = Local::now().timestamp();
        let pending = self.list(false)?;
        let overdue = pending
            .iter()
            .filter(|t| t.recurrence.is_none() && t.due_at.map(|d| d < now_ts).unwrap_or(false))
            .count();
        Ok((pending.len(), overdue))
    }
}

/// Construit le rapport interne reinjecte dans la conversation, sur le
/// meme principe qu'un compte rendu de mission (`mission.rs`) : Nestor
/// l'annonce avec ses propres mots plutot qu'un texte brut pousse au TTS.
pub fn build_reminder_report(due: &[Todo]) -> String {
    let items: Vec<String> = due
        .iter()
        .map(|t| match (&t.recurrence, t.due_at) {
            (Some(_), _) => t.title.clone(),
            (None, Some(due_at)) => format!("{} (echeance {})", t.title, format_due(due_at)),
            (None, None) => t.title.clone(),
        })
        .collect();

    format!(
        "[Rappel interne : {} tache(s) a relancer aupres de l'utilisateur : {}.] \
Mentionne-les naturellement, en une ou deux phrases, sans lire une liste a voix haute.",
        items.len(),
        items.join(" ; ")
    )
}

/// Boucle de fond : verifie periodiquement s'il y a des taches a relancer et,
/// si quelqu'un est bien connecte pour l'entendre (sinon ce serait du quota
/// Claude depense pour personne) et qu'on n'est pas en heures calmes,
/// reinjecte un rappel dans la conversation. C'est la version minimale de la
/// « boucle proactive » de `.agent/VISION.md` : seulement les taches, pas
/// encore position/agenda/mails ni canal hors appel.
pub fn spawn_proactive_loop(
    events_tx: tokio::sync::broadcast::Sender<crate::protocol::ServerEvent>,
    brain: std::sync::Arc<crate::brain::NestorBrain>,
    config: std::sync::Arc<crate::config::Config>,
    todos: std::sync::Arc<TodoStore>,
) {
    const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CHECK_INTERVAL);
        loop {
            interval.tick().await;

            if config.quiet_hours.contains(Local::now().time()) {
                continue;
            }
            if events_tx.receiver_count() == 0 {
                // Personne pour entendre le rappel : on le laisse pour la
                // prochaine connexion (nudge de debut de conversation).
                continue;
            }

            let due = match todos.due_now() {
                Ok(due) => due,
                Err(err) => {
                    tracing::error!(?err, "echec de lecture des taches dues");
                    continue;
                }
            };
            if due.is_empty() {
                continue;
            }

            let ids: Vec<i64> = due.iter().map(|t| t.id).collect();
            let report = build_reminder_report(&due);
            tracing::info!(count = due.len(), "relance proactive de taches");

            if let Err(err) = brain.send_internal_report(&report).await {
                tracing::error!(?err, "echec d'envoi du rappel proactif");
                continue;
            }
            if let Err(err) = todos.mark_notified(&ids) {
                tracing::error!(?err, "echec de marquage des taches relancees");
            }
        }
    });
}

/// Formate une echeance pour l'affichage/prononciation (date locale JJ/MM).
pub fn format_due(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%d/%m %H:%M").to_string())
        .unwrap_or_else(|| "date inconnue".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> TodoStore {
        TodoStore::open(&std::path::PathBuf::from(format!(
            "/tmp/nestord-todo-test-{}.db",
            uuid_like()
        )))
        .unwrap()
    }

    fn uuid_like() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[test]
    fn ajoute_et_liste() {
        let store = store();
        store.add("Relire le contrat", None, None, None).unwrap();
        let items = store.list(false).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Relire le contrat");
    }

    #[test]
    fn tache_ponctuelle_en_retard_est_due() {
        let store = store();
        let past = Local::now().timestamp() - 3600;
        store.add("Payer la facture", None, Some(past), None).unwrap();
        let due = store.due_now().unwrap();
        assert_eq!(due.len(), 1);
    }

    #[test]
    fn completer_une_tache_ponctuelle_la_retire() {
        let store = store();
        let id = store.add("Sortir les poubelles", None, None, None).unwrap();
        store.complete(id).unwrap();
        let items = store.list(false).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn recurrence_quotidienne_reapparait_apres_cooldown() {
        let store = store();
        let id = store
            .add("Arroser les plantes", None, None, Some(Recurrence::Daily))
            .unwrap();
        assert_eq!(store.due_now().unwrap().len(), 1);
        store.complete(id).unwrap();
        assert!(store.due_now().unwrap().is_empty(), "doit etre en cooldown juste apres");
    }

    #[test]
    fn parse_recurrence() {
        assert_eq!(Recurrence::parse("daily"), Some(Recurrence::Daily));
        assert_eq!(Recurrence::parse("weekly:lun"), Some(Recurrence::Weekly(chrono::Weekday::Mon)));
        assert_eq!(Recurrence::parse("monthly:15"), Some(Recurrence::Monthly(15)));
        assert_eq!(Recurrence::parse("n'importe quoi"), None);
    }
}
