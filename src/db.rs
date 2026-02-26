use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

use crate::models::{ConsolidatedMemory, Memory, Skill, Stats};

pub fn open_raw_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
            id INTEGER PRIMARY KEY,
            content TEXT NOT NULL,
            type TEXT NOT NULL DEFAULT 'observation',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
            access_count INTEGER NOT NULL DEFAULT 0,
            consolidated INTEGER NOT NULL DEFAULT 0,
            importance REAL NOT NULL DEFAULT 0.5,
            session_id TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(content, type, content=memories, content_rowid=id, tokenize='porter unicode61');
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, type) VALUES (new.id, new.content, new.type);
        END;
        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, type) VALUES('delete', old.id, old.content, old.type);
        END;
        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, type) VALUES('delete', old.id, old.content, old.type);
            INSERT INTO memories_fts(rowid, content, type) VALUES (new.id, new.content, new.type);
        END;",
    )?;
    Ok(conn)
}

pub fn open_consolidated_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS consolidated (
            id INTEGER PRIMARY KEY,
            content TEXT NOT NULL,
            type TEXT NOT NULL,
            source_ids TEXT NOT NULL DEFAULT '[]',
            confidence REAL NOT NULL DEFAULT 0.5,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            access_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS skills (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            content TEXT NOT NULL,
            source_ids TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

pub fn save_memory(conn: &Connection, content: &str, mem_type: &str, session_id: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO memories (content, type, session_id) VALUES (?1, ?2, ?3)",
        params![content, mem_type, session_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn recall_memories(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Memory>> {
    // Preprocess query: add prefix matching (*) to each term for fuzzy matching
    let fts_query = query
        .split_whitespace()
        .map(|word| {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            if clean.is_empty() { String::new() } else { format!("{}*", clean) }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" OR ");

    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, m.type, m.created_at, m.accessed_at,
                m.access_count, m.consolidated, m.importance, m.session_id
         FROM memories_fts f
         JOIN memories m ON f.rowid = m.id
         WHERE memories_fts MATCH ?1
         ORDER BY f.rank * (1.0 / (1.0 + (julianday('now') - julianday(m.accessed_at))))
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
        Ok(Memory {
            id: row.get(0)?,
            content: row.get(1)?,
            r#type: row.get(2)?,
            created_at: row.get(3)?,
            accessed_at: row.get(4)?,
            access_count: row.get(5)?,
            consolidated: row.get::<_, i64>(6)? != 0,
            importance: row.get(7)?,
            session_id: row.get(8)?,
        })
    })?;
    let mut memories = Vec::new();
    for row in rows {
        let m = row?;
        conn.execute(
            "UPDATE memories SET accessed_at = datetime('now'), access_count = access_count + 1 WHERE id = ?1",
            params![m.id],
        )?;
        memories.push(m);
    }
    Ok(memories)
}

pub fn get_unconsolidated_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE consolidated = 0",
        [],
        |row| row.get(0),
    )?)
}

pub fn get_unconsolidated_memories(conn: &Connection) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, created_at, accessed_at, access_count, consolidated, importance, session_id
         FROM memories WHERE consolidated = 0 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Memory {
            id: row.get(0)?,
            content: row.get(1)?,
            r#type: row.get(2)?,
            created_at: row.get(3)?,
            accessed_at: row.get(4)?,
            access_count: row.get(5)?,
            consolidated: row.get::<_, i64>(6)? != 0,
            importance: row.get(7)?,
            session_id: row.get(8)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn mark_consolidated(conn: &Connection, ids: &[i64]) -> Result<()> {
    for id in ids {
        conn.execute("UPDATE memories SET consolidated = 1 WHERE id = ?1", params![id])?;
    }
    Ok(())
}

pub fn get_all_consolidated(conn: &Connection) -> Result<Vec<ConsolidatedMemory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, source_ids, confidence, created_at, updated_at, access_count
         FROM consolidated ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let source_ids_str: String = row.get(3)?;
        let source_ids: Vec<i64> = serde_json::from_str(&source_ids_str).unwrap_or_default();
        Ok(ConsolidatedMemory {
            id: row.get(0)?,
            content: row.get(1)?,
            r#type: row.get(2)?,
            source_ids,
            confidence: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            access_count: row.get(7)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn insert_consolidated(conn: &Connection, content: &str, mem_type: &str, source_ids: &[i64], confidence: f64) -> Result<i64> {
    let source_json = serde_json::to_string(source_ids)?;
    conn.execute(
        "INSERT INTO consolidated (content, type, source_ids, confidence) VALUES (?1, ?2, ?3, ?4)",
        params![content, mem_type, source_json, confidence],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Check if consolidated DB already contains an entry with this exact content.
pub fn consolidated_content_exists(conn: &Connection, content: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM consolidated WHERE content = ?1",
        params![content],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Count total consolidated entries.
pub fn get_consolidated_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM consolidated", [], |row| row.get(0))?)
}

pub fn remove_consolidated(conn: &Connection, ids: &[i64]) -> Result<()> {
    for id in ids {
        conn.execute("DELETE FROM consolidated WHERE id = ?1", params![id])?;
    }
    Ok(())
}

pub fn upsert_skill(conn: &Connection, name: &str, content: &str, source_ids: &[i64]) -> Result<()> {
    let source_json = serde_json::to_string(source_ids)?;
    conn.execute(
        "INSERT INTO skills (name, content, source_ids, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(name) DO UPDATE SET content = ?2, source_ids = ?3, updated_at = datetime('now')",
        params![name, content, source_json],
    )?;
    Ok(())
}

pub fn get_all_skills(conn: &Connection) -> Result<Vec<Skill>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, content, source_ids, updated_at FROM skills ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        let source_ids_str: String = row.get(3)?;
        let source_ids: Vec<i64> = serde_json::from_str(&source_ids_str).unwrap_or_default();
        Ok(Skill {
            id: row.get(0)?,
            name: row.get(1)?,
            content: row.get(2)?,
            source_ids,
            updated_at: row.get(4)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_stats(raw_conn: &Connection, cons_conn: &Connection) -> Result<Stats> {
    let raw_count: i64 = raw_conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    let unconsolidated_count: i64 = raw_conn.query_row("SELECT COUNT(*) FROM memories WHERE consolidated = 0", [], |r| r.get(0))?;
    let consolidated_count: i64 = cons_conn.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0))?;
    let skill_count: i64 = cons_conn.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))?;
    let last_sleep = get_meta(cons_conn, "last_sleep")?;
    Ok(Stats { raw_count, unconsolidated_count, consolidated_count, skill_count, last_sleep })
}

pub fn delete_memory(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    Ok(())
}
