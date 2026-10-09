//! Memoire longue de Nestor (`docs/memory.md`) : un graphe de faits en SQLite.
//!
//! Un seul type de noeud (`kind` discrimine : person, preference, project,
//! decision, pitfall, mission, code, place, fact), des aretes typees entre
//! noeuds, une recherche lexicale FTS5 (tokenizer unicode61 sans diacritiques,
//! pour le francais) suivie d'un saut dans le graphe pour ramener le *pourquoi*.
//!
//! Regles d'ecriture : dedoublonnage par titre normalise dans un meme contexte
//! (mise a jour et `confidence` qui monte plutot qu'un doublon) ; contradiction
//! par remplacement (`valid_until` sur l'ancien, arete `remplace`) ; oubli par
//! pierre tombale (`forgotten`). Rien n'est jamais reecrit dans l'histoire.
//!
//! Regle de lecture : la memoire ne se deverse jamais dans le prompt. Le modele
//! tire par `memory_search` (budget `BUDGET_CHARS` par appel) ; seule une fiche
//! de dix lignes est ajoutee au prompt systeme au demarrage (`startup_sheet`).
//!
//! Stockage SQLite plutot qu'Elasticsearch : aucun service a faire vivre, un
//! fichier sauvegardable, et l'interface MCP est la meme si l'on change.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::clock::now_ms;

/// Corps d'un noeud : un resume prononcable et un pointeur, pas un document.
pub const BODY_MAX_CHARS: usize = 500;
/// Memoire injectee par recherche, au plus (une dizaine de faits).
pub const BUDGET_CHARS: usize = 1_500;
/// Fiche de demarrage : dix lignes au plus.
pub const SHEET_MAX_LINES: usize = 10;

pub const KINDS: [&str; 9] = ["person", "preference", "project", "decision", "pitfall", "mission", "code", "place", "fact"];
pub const RELATIONS: [&str; 8] =
    ["concerne", "decide_pour", "remplace", "contredit", "cause", "fait_partie_de", "produit_par", "mentionne"];

fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/nestord/memory.db")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Node {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub context: String,
    pub tags: String,
    pub source: String,
    pub confidence: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub valid_until: Option<i64>,
}

impl Node {
    /// Une ligne pour l'assistant : identifiant, type, contexte, titre, corps.
    pub fn render(&self) -> String {
        let mut line = format!("#{} [{}|{}] {}", self.id, self.kind, self.context, self.title);
        if !self.body.is_empty() {
            line.push_str(" — ");
            line.push_str(&self.body);
        }
        if self.valid_until.is_some() {
            line.push_str(" (perime)");
        }
        line
    }
}

/// Fait a ecrire.
#[derive(Debug, Clone, Default)]
pub struct NewNode {
    pub kind: String,
    pub title: String,
    pub body: String,
    pub context: String,
    pub tags: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    pub id: i64,
    /// Un noeud equivalent existait : mis a jour, confiance augmentee.
    pub updated: bool,
}

pub struct MemoryStore {
    conn: Mutex<Connection>,
}

static GLOBAL: OnceLock<Arc<MemoryStore>> = OnceLock::new();

pub fn init(store: Arc<MemoryStore>) {
    let _ = GLOBAL.set(store);
}

pub fn global() -> Option<&'static Arc<MemoryStore>> {
    GLOBAL.get()
}

fn normalize_title(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn truncate_chars(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Requete FTS5 tolerante : chaque mot compte, aucun n'est obligatoire.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(|w| format!("\"{}\"", w.replace('"', "")))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

impl MemoryStore {
    pub fn open_default() -> Result<Self> {
        Self::open(&default_db_path())
    }

    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                title_norm TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                context TEXT NOT NULL DEFAULT 'perso',
                tags TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT 'conversation',
                confidence INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                valid_until INTEGER,
                forgotten INTEGER NOT NULL DEFAULT 0,
                recalled INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS nodes_lookup ON nodes(context, title_norm);
            CREATE TABLE IF NOT EXISTS edges (
                from_id INTEGER NOT NULL,
                to_id INTEGER NOT NULL,
                relation TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                PRIMARY KEY (from_id, to_id, relation)
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
                title, body, tags, content='nodes', content_rowid='id',
                tokenize='unicode61 remove_diacritics 2'
            );
            CREATE TRIGGER IF NOT EXISTS nodes_ai AFTER INSERT ON nodes BEGIN
                INSERT INTO nodes_fts(rowid, title, body, tags) VALUES (new.id, new.title, new.body, new.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS nodes_ad AFTER DELETE ON nodes BEGIN
                INSERT INTO nodes_fts(nodes_fts, rowid, title, body, tags) VALUES ('delete', old.id, old.title, old.body, old.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS nodes_au AFTER UPDATE OF title, body, tags ON nodes BEGIN
                INSERT INTO nodes_fts(nodes_fts, rowid, title, body, tags) VALUES ('delete', old.id, old.title, old.body, old.tags);
                INSERT INTO nodes_fts(rowid, title, body, tags) VALUES (new.id, new.title, new.body, new.tags);
            END;",
        )
        .context("schema de la memoire")?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Ecrit un fait. Un noeud de meme titre (normalise) dans le meme contexte,
    /// encore valide, est mis a jour et gagne en confiance au lieu d'etre double.
    /// `replaces` : le noeud contredit recoit `valid_until` et une arete `remplace`.
    pub fn write(&self, new: NewNode, replaces: Option<i64>, links: &[(i64, String)]) -> Result<WriteOutcome> {
        let kind = if KINDS.contains(&new.kind.as_str()) { new.kind.clone() } else { "fact".to_string() };
        let title = truncate_chars(&new.title, 120);
        anyhow::ensure!(!title.is_empty(), "titre vide");
        let body = truncate_chars(&new.body, BODY_MAX_CHARS);
        let context = if new.context.trim().is_empty() { "perso".to_string() } else { new.context.trim().to_string() };
        let tags = new.tags.split(',').map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(",");
        let source = if new.source.trim().is_empty() { "conversation".to_string() } else { new.source.trim().to_string() };
        let now = now_ms() as i64;
        let norm = normalize_title(&title);

        let conn = self.conn.lock().unwrap();
        let existing: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT id, body, tags FROM nodes WHERE context = ?1 AND title_norm = ?2 AND forgotten = 0 AND valid_until IS NULL",
                params![context, norm],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (id, updated) = match existing {
            Some((id, old_body, old_tags)) => {
                let body = if body.is_empty() { old_body } else { body };
                let mut merged: Vec<&str> = old_tags.split(',').filter(|t| !t.is_empty()).collect();
                for tag in tags.split(',').filter(|t| !t.is_empty()) {
                    if !merged.contains(&tag) {
                        merged.push(tag);
                    }
                }
                conn.execute(
                    "UPDATE nodes SET body = ?1, tags = ?2, source = ?3, confidence = confidence + 1, updated_at = ?4 WHERE id = ?5",
                    params![body, merged.join(","), source, now, id],
                )?;
                (id, true)
            }
            None => {
                conn.execute(
                    "INSERT INTO nodes (kind, title, title_norm, body, context, tags, source, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![kind, title, norm, body, context, tags, source, now],
                )?;
                (conn.last_insert_rowid(), false)
            }
        };
        if let Some(old) = replaces.filter(|old| *old != id) {
            conn.execute("UPDATE nodes SET valid_until = ?1, updated_at = ?1 WHERE id = ?2 AND valid_until IS NULL", params![now, old])?;
            conn.execute(
                "INSERT OR IGNORE INTO edges (from_id, to_id, relation, context, source, created_at) VALUES (?1, ?2, 'remplace', ?3, ?4, ?5)",
                params![id, old, context, source, now],
            )?;
        }
        for (target, relation) in links {
            let relation = if RELATIONS.contains(&relation.as_str()) { relation.as_str() } else { "concerne" };
            conn.execute(
                "INSERT OR IGNORE INTO edges (from_id, to_id, relation, context, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, target, relation, context, source, now],
            )?;
        }
        Ok(WriteOutcome { id, updated })
    }

    /// Pierre tombale : le noeud sort de toute recuperation. `false` s'il est inconnu.
    pub fn forget(&self, id: i64) -> Result<bool> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE nodes SET forgotten = 1, updated_at = ?1 WHERE id = ?2 AND forgotten = 0",
            params![now_ms() as i64, id],
        )?;
        Ok(changed > 0)
    }

    /// Identifiant du fait valide portant ce titre (normalise) dans ce contexte.
    pub fn find_by_title(&self, context: &str, title: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT id FROM nodes WHERE context = ?1 AND title_norm = ?2 AND forgotten = 0 AND valid_until IS NULL",
                params![context, normalize_title(title)],
                |row| row.get(0),
            )
            .optional()?)
    }

    #[cfg(test)]
    pub fn get(&self, id: i64) -> Result<Option<Node>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("{SELECT_NODE} FROM nodes WHERE id = ?1 AND forgotten = 0"))?;
        Ok(stmt.query_row(params![id], row_to_node).optional()?)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM nodes WHERE forgotten = 0 AND valid_until IS NULL", [], |r| r.get(0))?)
    }

    /// Recherche lexicale, un saut de graphe depuis les meilleurs resultats,
    /// reclassement (pertinence, confiance, fraicheur, contexte) et troncature au
    /// budget en preferant la diversite des types. Les noeuds perimes sont ecartes
    /// sauf `include_history`.
    pub fn search(&self, query: &str, context: Option<&str>, kinds: &[String], limit: usize, include_history: bool) -> Result<Vec<Node>> {
        let Some(fts) = fts_query(query) else { return Ok(Vec::new()) };
        let now = now_ms() as i64;
        let conn = self.conn.lock().unwrap();

        // 1. Points d'entree : bm25 (plus petit = meilleur).
        let mut stmt = conn.prepare(&format!(
            "{SELECT_NODE}, bm25(nodes_fts) AS rank FROM nodes_fts JOIN nodes ON nodes.id = nodes_fts.rowid
             WHERE nodes_fts MATCH ?1 AND nodes.forgotten = 0 ORDER BY rank LIMIT 30"
        ))?;
        let mut scored: Vec<(f64, Node)> = stmt
            .query_map(params![fts], |row| Ok((row.get::<_, f64>(11)?, row_to_node(row)?)))?
            .collect::<std::result::Result<_, _>>()?;

        // 2. Un saut de graphe depuis les trois meilleurs : le pourquoi d'une decision.
        let seeds: Vec<i64> = scored.iter().take(3).map(|(_, n)| n.id).collect();
        for seed in seeds {
            let mut neighbours = conn.prepare(&format!(
                "{SELECT_NODE} FROM nodes WHERE forgotten = 0 AND id IN (
                    SELECT to_id FROM edges WHERE from_id = ?1 UNION SELECT from_id FROM edges WHERE to_id = ?1
                 ) LIMIT 8"
            ))?;
            for node in neighbours.query_map(params![seed], row_to_node)?.flatten() {
                if !scored.iter().any(|(_, n)| n.id == node.id) {
                    scored.push((-0.5, node));
                }
            }
        }
        drop(stmt);

        // 3. Filtres et reclassement.
        let day_ms = 86_400_000.0;
        let mut ranked: Vec<(f64, Node)> = scored
            .into_iter()
            .filter(|(_, n)| include_history || n.valid_until.is_none())
            .filter(|(_, n)| kinds.is_empty() || kinds.iter().any(|k| k == &n.kind))
            .map(|(rank, n)| {
                let relevance = 1.0 / (1.0 + rank.abs());
                let age_days = ((now - n.updated_at).max(0) as f64) / day_ms;
                let freshness = 1.0 / (1.0 + age_days / 180.0);
                let confidence = 1.0 + (n.confidence as f64).ln();
                let context_match = match context {
                    Some(c) if n.context == c => 1.5,
                    Some(_) => 0.7,
                    None => 1.0,
                };
                let expired = if n.valid_until.is_some() { 0.3 } else { 1.0 };
                (relevance * freshness * confidence * context_match * expired, n)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // 4. Troncature au budget, diversite des types d'abord (deux par type au premier tour).
        let mut chosen: Vec<Node> = Vec::new();
        let mut used = 0usize;
        let mut per_kind: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let budget_ok = |used: usize, n: &Node| used + n.render().chars().count() + 1 <= BUDGET_CHARS;
        for pass in 0..2 {
            for (_, node) in &ranked {
                if chosen.len() >= limit || chosen.iter().any(|c| c.id == node.id) {
                    continue;
                }
                let seen = per_kind.get(&node.kind).copied().unwrap_or(0);
                if pass == 0 && seen >= 2 {
                    continue;
                }
                if !budget_ok(used, node) {
                    continue;
                }
                used += node.render().chars().count() + 1;
                *per_kind.entry(node.kind.clone()).or_default() += 1;
                chosen.push(node.clone());
            }
        }
        if !chosen.is_empty() {
            let ids: Vec<String> = chosen.iter().map(|n| n.id.to_string()).collect();
            conn.execute(&format!("UPDATE nodes SET recalled = recalled + 1 WHERE id IN ({})", ids.join(",")), [])?;
        }
        Ok(chosen)
    }

    /// Fiche de demarrage : identite, preferences permanentes, projets, par confiance.
    pub fn startup_sheet(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "{SELECT_NODE} FROM nodes WHERE forgotten = 0 AND valid_until IS NULL AND kind IN ('person', 'preference', 'project')
             ORDER BY confidence DESC, updated_at DESC LIMIT ?1"
        ))?;
        let lines: Vec<String> = stmt
            .query_map(params![SHEET_MAX_LINES as i64], row_to_node)?
            .flatten()
            .map(|n| {
                let body = truncate_chars(&n.body, 90);
                if body.is_empty() { format!("- {}", n.title) } else { format!("- {} : {body}", n.title) }
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

const SELECT_NODE: &str = "SELECT nodes.id, nodes.kind, nodes.title, nodes.body, nodes.context, nodes.tags, nodes.source, \
nodes.confidence, nodes.created_at, nodes.updated_at, nodes.valid_until";

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    Ok(Node {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        context: row.get(4)?,
        tags: row.get(5)?,
        source: row.get(6)?,
        confidence: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        valid_until: row.get(10)?,
    })
}

/// Capture sans modele : le compte rendu d'une mission terminee devient un noeud
/// `mission` (titre = objet, corps = resume borne), relie a rien pour l'instant.
pub fn remember_mission(id: u64, description: &str, summary: &str) {
    let Some(store) = global() else { return };
    let node = NewNode {
        kind: "mission".to_string(),
        title: truncate_chars(description, 120),
        body: summary.to_string(),
        context: "missions".to_string(),
        tags: String::new(),
        source: format!("mission:{id}"),
    };
    if let Err(err) = store.write(node, None, &[]) {
        tracing::warn!(?err, "compte rendu de mission non memorise");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MemoryStore {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("nestord-memory-test-{}-{}-{seq}.db", std::process::id(), now_ms()));
        let _ = std::fs::remove_file(&path);
        MemoryStore::open(&path).unwrap()
    }

    fn node(kind: &str, title: &str, body: &str, context: &str) -> NewNode {
        NewNode { kind: kind.into(), title: title.into(), body: body.into(), context: context.into(), ..Default::default() }
    }

    #[test]
    fn ecriture_dedoublonnee_et_confiance() {
        let store = store();
        let first = store.write(node("preference", "Pas d'attribution IA dans les commits", "", "projet:nestor"), None, &[]).unwrap();
        assert!(!first.updated);
        let again = store.write(node("preference", "  pas d'attribution ia  dans les commits ", "seul le trailer Co-authored-by", "projet:nestor"), None, &[]).unwrap();
        assert_eq!(again.id, first.id);
        assert!(again.updated);
        let saved = store.get(first.id).unwrap().unwrap();
        assert_eq!(saved.confidence, 2);
        assert_eq!(saved.body, "seul le trailer Co-authored-by");
        assert_eq!(store.count().unwrap(), 1);
        // Meme titre dans un autre contexte : un autre fait.
        let other = store.write(node("preference", "Pas d'attribution IA dans les commits", "", "perso"), None, &[]).unwrap();
        assert_ne!(other.id, first.id);
    }

    #[test]
    fn recherche_lexicale_avec_saut_de_graphe_et_pourquoi() {
        let store = store();
        let pitfall = store
            .write(node("pitfall", "Kokoro v1.0 n'a qu'une voix francaise, feminine", "", "projet:nestor"), None, &[])
            .unwrap();
        let decision = store
            .write(
                node("decision", "Piper plutot que Kokoro pour la voix de Nestor", "voix masculine fr_FR-upmc-medium", "projet:nestor"),
                None,
                &[(pitfall.id, "cause".to_string())],
            )
            .unwrap();
        store.write(node("fact", "Le chat s'appelle Moustache", "", "perso"), None, &[]).unwrap();

        let hits = store.search("pourquoi Piper ?", Some("projet:nestor"), &[], 5, false).unwrap();
        let ids: Vec<i64> = hits.iter().map(|n| n.id).collect();
        assert_eq!(ids[0], decision.id, "la decision est le point d'entree");
        assert!(ids.contains(&pitfall.id), "le piege arrive par l'arete, sans etre nomme");
        assert!(!hits.iter().any(|n| n.title.contains("Moustache")));
        // Accents et casse ignores.
        let hits = store.search("VOIX FRANÇAISE", None, &[], 5, false).unwrap();
        assert!(hits.iter().any(|n| n.id == pitfall.id));
        // Filtre par type.
        let hits = store.search("Piper Kokoro", None, &["pitfall".to_string()], 5, false).unwrap();
        assert!(hits.iter().all(|n| n.kind == "pitfall"));
        // Requete sans mot utile : rien, sans erreur.
        assert!(store.search("?", None, &[], 5, false).unwrap().is_empty());
    }

    #[test]
    fn contradiction_remplace_sans_reecrire_et_oubli() {
        let store = store();
        let old = store.write(node("preference", "Reunion d'equipe le lundi", "", "perso"), None, &[]).unwrap();
        let new = store.write(node("preference", "Reunion d'equipe le mardi", "depuis octobre", "perso"), Some(old.id), &[]).unwrap();
        let hits = store.search("reunion equipe", None, &[], 5, false).unwrap();
        assert_eq!(hits.iter().map(|n| n.id).collect::<Vec<_>>(), [new.id], "le perime est ecarte");
        let history = store.search("reunion equipe", None, &[], 5, true).unwrap();
        assert!(history.iter().any(|n| n.id == old.id && n.valid_until.is_some()));
        assert!(history.iter().any(|n| n.id == old.id && n.render().ends_with("(perime)")));

        assert!(store.forget(new.id).unwrap());
        assert!(!store.forget(new.id).unwrap());
        assert!(store.search("reunion equipe", None, &[], 5, false).unwrap().is_empty());
        assert!(store.get(new.id).unwrap().is_none());
    }

    #[test]
    fn budget_et_fiche_de_demarrage() {
        let store = store();
        let long_body = "x".repeat(600);
        for i in 0..20 {
            store.write(node("fact", &format!("Fait numero {i} sur le projet"), &long_body, "projet:nestor"), None, &[]).unwrap();
        }
        let hits = store.search("fait projet", None, &[], 20, false).unwrap();
        let total: usize = hits.iter().map(|n| n.render().chars().count() + 1).sum();
        assert!(total <= BUDGET_CHARS, "budget depasse : {total}");
        assert!(hits.len() >= 2 && hits.len() < 20);
        assert!(hits[0].body.chars().count() <= BODY_MAX_CHARS);

        store.write(node("person", "Monsieur se prenomme Philippe", "", "perso"), None, &[]).unwrap();
        store.write(node("preference", "Vouvoiement", "toujours", "perso"), None, &[]).unwrap();
        let sheet = store.startup_sheet().unwrap();
        assert!(sheet.contains("- Monsieur se prenomme Philippe"));
        assert!(sheet.contains("- Vouvoiement : toujours"));
        assert!(!sheet.contains("Fait numero"), "les faits ne vont pas dans la fiche");
        assert!(sheet.lines().count() <= SHEET_MAX_LINES);
    }
}
