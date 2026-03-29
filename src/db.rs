use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

use crate::models::{ConsolidatedMemory, Entity, Memory, Relationship, Skill, Stats};

pub fn open_raw_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
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
            session_id TEXT,
            entity_ids TEXT NOT NULL DEFAULT '[]'
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

    // Migrate: add entity_ids column if missing
    let has_entity_ids = conn
        .prepare("SELECT entity_ids FROM memories LIMIT 0")
        .is_ok();
    if !has_entity_ids {
        conn.execute_batch("ALTER TABLE memories ADD COLUMN entity_ids TEXT NOT NULL DEFAULT '[]';")?;
    }

    // Create entities table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entities (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            entity_type TEXT NOT NULL,
            description TEXT,
            confidence REAL DEFAULT 0.5,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            access_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
            name, description, content=entities, content_rowid=id, tokenize='porter unicode61'
        );
        CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
            INSERT INTO entities_fts(rowid, name, description) VALUES (new.id, new.name, new.description);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
            INSERT INTO entities_fts(entities_fts, rowid, name, description) VALUES('delete', old.id, old.name, old.description);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities BEGIN
            INSERT INTO entities_fts(entities_fts, rowid, name, description) VALUES('delete', old.id, old.name, old.description);
            INSERT INTO entities_fts(rowid, name, description) VALUES (new.id, new.name, new.description);
        END;",
    )?;

    // Create relationships table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS relationships (
            id INTEGER PRIMARY KEY,
            source_entity_id INTEGER NOT NULL,
            target_entity_id INTEGER NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            evidence_ids TEXT NOT NULL DEFAULT '[]',
            confidence REAL DEFAULT 0.5,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (source_entity_id) REFERENCES entities(id),
            FOREIGN KEY (target_entity_id) REFERENCES entities(id)
        );
        CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_entity_id);
        CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_entity_id);
        CREATE INDEX IF NOT EXISTS idx_relationships_type ON relationships(relation_type);",
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
            access_count INTEGER NOT NULL DEFAULT 0,
            entity_ids TEXT NOT NULL DEFAULT '[]',
            active INTEGER NOT NULL DEFAULT 1,
            superseded_by INTEGER
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS consolidated_fts USING fts5(content, type, content=consolidated, content_rowid=id, tokenize='porter unicode61');
        CREATE TRIGGER IF NOT EXISTS consolidated_ai AFTER INSERT ON consolidated BEGIN
            INSERT INTO consolidated_fts(rowid, content, type) VALUES (new.id, new.content, new.type);
        END;
        CREATE TRIGGER IF NOT EXISTS consolidated_ad AFTER DELETE ON consolidated BEGIN
            INSERT INTO consolidated_fts(consolidated_fts, rowid, content, type) VALUES('delete', old.id, old.content, old.type);
        END;
        CREATE TRIGGER IF NOT EXISTS consolidated_au AFTER UPDATE ON consolidated BEGIN
            INSERT INTO consolidated_fts(consolidated_fts, rowid, content, type) VALUES('delete', old.id, old.content, old.type);
            INSERT INTO consolidated_fts(rowid, content, type) VALUES (new.id, new.content, new.type);
        END;
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

    // Migrate: add entity_ids column if missing
    let has_entity_ids = conn
        .prepare("SELECT entity_ids FROM consolidated LIMIT 0")
        .is_ok();
    if !has_entity_ids {
        conn.execute_batch("ALTER TABLE consolidated ADD COLUMN entity_ids TEXT NOT NULL DEFAULT '[]';")?;
    }

    let has_active = conn.prepare("SELECT active FROM consolidated LIMIT 0").is_ok();
    if !has_active {
        conn.execute_batch("ALTER TABLE consolidated ADD COLUMN active INTEGER NOT NULL DEFAULT 1;")?;
    }

    let has_superseded_by = conn.prepare("SELECT superseded_by FROM consolidated LIMIT 0").is_ok();
    if !has_superseded_by {
        conn.execute_batch("ALTER TABLE consolidated ADD COLUMN superseded_by INTEGER;")?;
    }

    Ok(conn)
}

// --- Memory CRUD ---

pub fn save_memory(conn: &Connection, content: &str, mem_type: &str, session_id: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO memories (content, type, session_id) VALUES (?1, ?2, ?3)",
        params![content, mem_type, session_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn save_memory_with_entities(
    conn: &Connection,
    content: &str,
    mem_type: &str,
    session_id: &str,
    entity_ids: &[i64],
) -> Result<i64> {
    let entity_json = serde_json::to_string(entity_ids)?;
    conn.execute(
        "INSERT INTO memories (content, type, session_id, entity_ids) VALUES (?1, ?2, ?3, ?4)",
        params![content, mem_type, session_id, entity_json],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_memory_entities(conn: &Connection, id: i64, entity_ids: &[i64]) -> Result<()> {
    let entity_json = serde_json::to_string(entity_ids)?;
    conn.execute(
        "UPDATE memories SET entity_ids = ?1 WHERE id = ?2",
        params![entity_json, id],
    )?;
    Ok(())
}

pub fn update_memory_importance(conn: &Connection, id: i64, importance: f64) -> Result<()> {
    conn.execute(
        "UPDATE memories SET importance = ?1 WHERE id = ?2",
        params![importance, id],
    )?;
    Ok(())
}

pub fn recall_memories(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Memory>> {
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, m.type, m.created_at, m.accessed_at,
                m.access_count, m.consolidated, m.importance, m.session_id, m.entity_ids
         FROM memories_fts f
         JOIN memories m ON f.rowid = m.id
         WHERE memories_fts MATCH ?1
         ORDER BY bm25(memories_fts), m.importance DESC, m.access_count DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, candidate_limit(limit) as i64], |row| {
        let entity_ids_str: String = row.get(9)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_str).unwrap_or_default();
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
            entity_ids,
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
    Ok(focus_memory_results(memories, query, limit))
}

/// Recall memories by entity: find all memories referencing an entity and optionally its neighbors.
pub fn recall_by_entity(conn: &Connection, entity_name: &str, include_neighbors: bool, limit: usize) -> Result<Vec<Memory>> {
    // Find the entity
    let entity_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM entities WHERE name = ?1 COLLATE NOCASE",
            params![entity_name],
            |row| row.get(0),
        )
        .ok();

    let entity_id = match entity_id {
        Some(id) => id,
        None => return Ok(vec![]),
    };

    // Update entity access count
    conn.execute(
        "UPDATE entities SET access_count = access_count + 1, updated_at = datetime('now') WHERE id = ?1",
        params![entity_id],
    )?;

    let mut entity_ids = vec![entity_id];

    if include_neighbors {
        let mut stmt = conn.prepare(
            "SELECT target_entity_id FROM relationships WHERE source_entity_id = ?1
             UNION
             SELECT source_entity_id FROM relationships WHERE target_entity_id = ?1",
        )?;
        let neighbor_ids: Vec<i64> = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        entity_ids.extend(neighbor_ids);
    }

    // Find memories referencing any of these entities
    let placeholders: Vec<String> = entity_ids.iter().map(|_| "?".to_string()).collect();
    // We use json_each to check if entity_ids array contains any of our target IDs
    let query = format!(
        "SELECT DISTINCT m.id, m.content, m.type, m.created_at, m.accessed_at,
                m.access_count, m.consolidated, m.importance, m.session_id, m.entity_ids
         FROM memories m, json_each(m.entity_ids) e
         WHERE e.value IN ({})
         ORDER BY m.accessed_at DESC
         LIMIT ?",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&query)?;
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = entity_ids
        .iter()
        .map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    param_values.push(Box::new(limit as i64));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        let entity_ids_str: String = row.get(9)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_str).unwrap_or_default();
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
            entity_ids,
        })
    })?;

    let memories: Vec<Memory> = rows.into_iter().map(|r| Ok(r?)).collect::<Result<_>>()?;
    Ok(focus_memory_results(memories, entity_name, limit))
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
        "SELECT id, content, type, created_at, accessed_at, access_count, consolidated, importance, session_id, entity_ids
         FROM memories WHERE consolidated = 0 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let entity_ids_str: String = row.get(9)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_str).unwrap_or_default();
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
            entity_ids,
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

pub fn delete_memory(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    Ok(())
}

// --- Entity CRUD ---

pub fn upsert_entity(conn: &Connection, name: &str, entity_type: &str, description: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT INTO entities (name, entity_type, description)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET
             entity_type = ?2,
             description = COALESCE(?3, entities.description),
             updated_at = datetime('now')",
        params![name, entity_type, description],
    )?;
    let id = conn.query_row(
        "SELECT id FROM entities WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn update_entity(conn: &Connection, name: &str, description: Option<&str>, confidence: f64) -> Result<()> {
    conn.execute(
        "UPDATE entities SET description = COALESCE(?2, description), confidence = ?3, updated_at = datetime('now')
         WHERE name = ?1 COLLATE NOCASE",
        params![name, description, confidence],
    )?;
    Ok(())
}

pub fn get_entity_by_name(conn: &Connection, name: &str) -> Result<Option<Entity>> {
    let result = conn.query_row(
        "SELECT id, name, entity_type, description, confidence, created_at, updated_at, access_count
         FROM entities WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |row| {
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                confidence: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                access_count: row.get(7)?,
            })
        },
    );
    match result {
        Ok(e) => Ok(Some(e)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_all_entities(conn: &Connection) -> Result<Vec<Entity>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, entity_type, description, confidence, created_at, updated_at, access_count
         FROM entities ORDER BY access_count DESC, updated_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Entity {
            id: row.get(0)?,
            name: row.get(1)?,
            entity_type: row.get(2)?,
            description: row.get(3)?,
            confidence: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            access_count: row.get(7)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn search_entities(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Entity>> {
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT e.id, e.name, e.entity_type, e.description, e.confidence, e.created_at, e.updated_at, e.access_count
         FROM entities_fts f
         JOIN entities e ON f.rowid = e.id
         WHERE entities_fts MATCH ?1
         ORDER BY f.rank
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
        Ok(Entity {
            id: row.get(0)?,
            name: row.get(1)?,
            entity_type: row.get(2)?,
            description: row.get(3)?,
            confidence: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            access_count: row.get(7)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn get_entity_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))?)
}

// --- Relationship CRUD ---

pub fn upsert_relationship(
    conn: &Connection,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
    evidence_id: i64,
    confidence: f64,
) -> Result<i64> {
    // Check for existing relationship
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, evidence_ids FROM relationships
             WHERE source_entity_id = ?1 AND target_entity_id = ?2 AND relation_type = ?3",
            params![source_id, target_id, relation_type],
            |row| Ok((row.get(0)?, row.get::<_, String>(1)?)),
        )
        .ok();

    if let Some((id, evidence_json)) = existing {
        let mut evidence: Vec<i64> = serde_json::from_str(&evidence_json).unwrap_or_default();
        if !evidence.contains(&evidence_id) {
            evidence.push(evidence_id);
        }
        let new_evidence = serde_json::to_string(&evidence)?;
        let new_weight = evidence.len() as f64;
        conn.execute(
            "UPDATE relationships SET evidence_ids = ?1, weight = ?2, confidence = MAX(confidence, ?3), updated_at = datetime('now')
             WHERE id = ?4",
            params![new_evidence, new_weight, confidence, id],
        )?;
        Ok(id)
    } else {
        let evidence_json = serde_json::to_string(&[evidence_id])?;
        conn.execute(
            "INSERT INTO relationships (source_entity_id, target_entity_id, relation_type, evidence_ids, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![source_id, target_id, relation_type, evidence_json, confidence],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

pub fn get_relationships_for_entity(conn: &Connection, entity_id: i64) -> Result<Vec<Relationship>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_entity_id, target_entity_id, relation_type, weight, evidence_ids, confidence, created_at, updated_at
         FROM relationships
         WHERE source_entity_id = ?1 OR target_entity_id = ?1
         ORDER BY weight DESC",
    )?;
    let rows = stmt.query_map(params![entity_id], |row| {
        let evidence_str: String = row.get(5)?;
        let evidence_ids: Vec<i64> = serde_json::from_str(&evidence_str).unwrap_or_default();
        Ok(Relationship {
            id: row.get(0)?,
            source_entity_id: row.get(1)?,
            target_entity_id: row.get(2)?,
            relation_type: row.get(3)?,
            weight: row.get(4)?,
            evidence_ids,
            confidence: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn get_all_relationships(conn: &Connection) -> Result<Vec<Relationship>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_entity_id, target_entity_id, relation_type, weight, evidence_ids, confidence, created_at, updated_at
         FROM relationships ORDER BY weight DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let evidence_str: String = row.get(5)?;
        let evidence_ids: Vec<i64> = serde_json::from_str(&evidence_str).unwrap_or_default();
        Ok(Relationship {
            id: row.get(0)?,
            source_entity_id: row.get(1)?,
            target_entity_id: row.get(2)?,
            relation_type: row.get(3)?,
            weight: row.get(4)?,
            evidence_ids,
            confidence: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn get_relationship_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?)
}

// --- Consolidated CRUD ---

pub fn get_all_consolidated(conn: &Connection) -> Result<Vec<ConsolidatedMemory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, source_ids, confidence, created_at, updated_at, access_count, active, superseded_by
         FROM consolidated WHERE active = 1 ORDER BY updated_at DESC",
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
            active: row.get::<_, i64>(8)? != 0,
            superseded_by: row.get(9)?,
        })
    })?;
    rows.into_iter().map(|r| Ok(r?)).collect()
}

pub fn search_consolidated(conn: &Connection, query: &str, limit: usize) -> Result<Vec<ConsolidatedMemory>> {
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.content, c.type, c.source_ids, c.confidence, c.created_at, c.updated_at, c.access_count, c.active, c.superseded_by
         FROM consolidated_fts f
         JOIN consolidated c ON f.rowid = c.id
         WHERE consolidated_fts MATCH ?1 AND c.active = 1
         ORDER BY bm25(consolidated_fts), c.confidence DESC, c.access_count DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, candidate_limit(limit) as i64], |row| {
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
            active: row.get::<_, i64>(8)? != 0,
            superseded_by: row.get(9)?,
        })
    })?;
    let memories: Vec<ConsolidatedMemory> = rows.into_iter().map(|r| Ok(r?)).collect::<Result<_>>()?;
    Ok(focus_consolidated_results(memories, query, limit))
}

pub fn insert_consolidated(conn: &Connection, content: &str, mem_type: &str, source_ids: &[i64], confidence: f64) -> Result<i64> {
    let source_json = serde_json::to_string(source_ids)?;
    conn.execute(
        "INSERT INTO consolidated (content, type, source_ids, confidence) VALUES (?1, ?2, ?3, ?4)",
        params![content, mem_type, source_json, confidence],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn consolidated_content_exists(conn: &Connection, content: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM consolidated WHERE content = ?1",
        params![content],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn get_consolidated_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM consolidated", [], |row| row.get(0))?)
}

pub fn update_consolidated(conn: &Connection, id: i64, content: &str) -> Result<bool> {
    let updated = conn.execute(
        "UPDATE consolidated SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![content, id],
    )?;
    Ok(updated > 0)
}

pub fn mark_consolidated_superseded(conn: &Connection, old_id: i64, new_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE consolidated SET active = 0, superseded_by = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![old_id, new_id],
    )?;
    Ok(())
}

pub fn remove_consolidated(conn: &Connection, ids: &[i64]) -> Result<()> {
    for id in ids {
        conn.execute("DELETE FROM consolidated WHERE id = ?1", params![id])?;
    }
    Ok(())
}

// --- Skills ---

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

// --- Meta ---

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

// --- Stats ---

pub fn get_stats(raw_conn: &Connection, cons_conn: &Connection) -> Result<Stats> {
    let raw_count: i64 = raw_conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    let unconsolidated_count: i64 = raw_conn.query_row("SELECT COUNT(*) FROM memories WHERE consolidated = 0", [], |r| r.get(0))?;
    let consolidated_count: i64 = cons_conn.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0))?;
    let skill_count: i64 = cons_conn.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))?;
    let entity_count: i64 = get_entity_count(raw_conn)?;
    let relationship_count: i64 = get_relationship_count(raw_conn)?;
    let last_sleep = get_meta(cons_conn, "last_sleep")?;
    Ok(Stats { raw_count, unconsolidated_count, consolidated_count, skill_count, entity_count, relationship_count, last_sleep })
}

// --- Helpers ---

fn candidate_limit(limit: usize) -> usize {
    std::cmp::max(limit.saturating_mul(4), limit.max(1))
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for word in query.split_whitespace() {
        let expanded = expand_camel_case(
            &word
                .chars()
                .filter(|c| {
                    c.is_alphanumeric()
                        || *c == '_'
                        || *c == '-'
                        || *c == '+'
                        || *c == '.'
                        || *c == '/'
                        || *c == '\\'
                        || *c == ':'
                })
                .collect::<String>(),
        );
        let normalized = normalize_query_term(
            &expanded.to_lowercase(),
        );
        push_query_term(&mut terms, normalized.clone());
        for piece in split_compound_term(&normalized) {
            push_query_term(&mut terms, normalize_query_term(piece));
        }
    }
    terms
}

fn split_compound_term(term: &str) -> impl Iterator<Item = &str> {
    term.split(|c| c == '-' || c == '_' || c == '+' || c == '.' || c == '/' || c == '\\' || c == ':')
        .filter(|piece| !piece.is_empty())
}

fn expand_camel_case(term: &str) -> String {
    let chars: Vec<char> = term.chars().collect();
    let mut expanded = String::with_capacity(term.len() + 4);
    for (idx, ch) in chars.iter().copied().enumerate() {
        if idx > 0 && ch.is_uppercase() {
            let prev = chars[idx - 1];
            let next = chars.get(idx + 1).copied();
            let camel_boundary = prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next.map(|c| c.is_lowercase()).unwrap_or(false));
            if camel_boundary {
                expanded.push('_');
            }
        }
        expanded.push(ch);
    }
    expanded
}

fn push_query_term(terms: &mut Vec<String>, term: String) {
    if !term.is_empty() && !terms.contains(&term) {
        terms.push(term);
    }
}

fn normalize_query_term(term: &str) -> String {
    match term {
        "prefer" | "prefers" | "preferred" | "preference" | "preferences" => "prefer".to_string(),
        _ if term.ends_with("ies") && term.len() > 4 => format!("{}y", &term[..term.len() - 3]),
        _ if term.ends_with('s') && term.len() > 4 && !term.ends_with("ss") => {
            term[..term.len() - 1].to_string()
        }
        _ => term.to_string(),
    }
}

fn is_routing_term(term: &str) -> bool {
    matches!(term, "prefer")
}

fn scoring_terms(query: &str) -> Vec<String> {
    let terms = query_terms(query);
    let informative: Vec<String> = terms
        .iter()
        .filter(|term| {
            !is_routing_term(term)
                && !term.contains('-')
                && !term.contains('_')
                && !term.contains('+')
                && !term.contains('.')
                && !term.contains('/')
                && !term.contains('\\')
                && !term.contains(':')
        })
        .cloned()
        .collect();
    if informative.is_empty() {
        terms.into_iter().filter(|term| !is_routing_term(term)).collect()
    } else {
        informative
    }
}

fn text_match_score(text: &str, terms: &[String], full_query: &str) -> usize {
    let lower = text.to_lowercase();
    let term_hits = terms.iter().filter(|term| lower.contains(term.as_str())).count();
    let literal_phrase_bonus = if !full_query.trim().is_empty() && lower.contains(&full_query.to_lowercase()) {
        1
    } else {
        0
    };
    let normalized_query = normalize_phrase(full_query);
    let normalized_text = normalize_phrase(text);
    let normalized_phrase_bonus = if !normalized_query.is_empty() && normalized_text.contains(&normalized_query) {
        2
    } else {
        0
    };
    term_hits * 10 + literal_phrase_bonus + normalized_phrase_bonus
}

fn normalize_phrase(text: &str) -> String {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|part| !part.is_empty())
        .flat_map(|part| {
            let expanded = expand_camel_case(part);
            let normalized = normalize_query_term(&expanded.to_lowercase());
            let pieces: Vec<String> = split_compound_term(&normalized).map(str::to_string).collect();
            if pieces.is_empty() {
                vec![normalized]
            } else {
                pieces
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_term_hits(text: &str, terms: &[String]) -> usize {
    let lower = text.to_lowercase();
    terms.iter().filter(|term| lower.contains(term.as_str())).count()
}

fn text_similarity(a: &str, b: &str) -> f64 {
    let a_tokens = content_tokens(a);
    let b_tokens = content_tokens(b);
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }
    let overlap = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();
    overlap as f64 / union as f64
}

fn content_tokens(text: &str) -> std::collections::HashSet<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|token| token.len() >= 3)
        .collect()
}

fn minimum_match_score(top_score: usize) -> usize {
    if top_score >= 30 {
        top_score
    } else if top_score <= 10 {
        top_score
    } else {
        ((top_score / 10) * 3).div_ceil(4) * 10
    }
}

fn focus_memory_results(memories: Vec<Memory>, query: &str, limit: usize) -> Vec<Memory> {
    if memories.is_empty() {
        return memories;
    }

    let terms = scoring_terms(query);
    if terms.is_empty() {
        return memories.into_iter().take(limit).collect();
    }

    let mut scored: Vec<(usize, f64, i64, i64, Memory)> = memories
        .into_iter()
        .map(|memory| {
            let score = text_match_score(&memory.content, &terms, query);
            (score, memory.importance, memory.access_count, memory.id, memory)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.cmp(&a.3))
    });

    let top_score = scored.first().map(|entry| entry.0).unwrap_or(0);
    if top_score == 0 {
        return Vec::new();
    }
    let min_score = minimum_match_score(top_score);
    let mut selected: Vec<Memory> = Vec::new();
    for (score, _importance, _access_count, _id, memory) in scored {
        if score < min_score {
            continue;
        }
        let is_redundant = selected.iter().any(|existing| {
            text_similarity(&existing.content, &memory.content) >= 0.45
                && query_term_hits(&existing.content, &terms) == query_term_hits(&memory.content, &terms)
        });
        if is_redundant {
            continue;
        }
        selected.push(memory);
        if selected.len() >= limit {
            break;
        }
    }
    selected
}

fn focus_consolidated_results(
    memories: Vec<ConsolidatedMemory>,
    query: &str,
    limit: usize,
) -> Vec<ConsolidatedMemory> {
    if memories.is_empty() {
        return memories;
    }

    let terms = scoring_terms(query);
    if terms.is_empty() {
        return memories.into_iter().take(limit).collect();
    }

    let mut scored: Vec<(usize, f64, i64, i64, ConsolidatedMemory)> = memories
        .into_iter()
        .map(|memory| {
            let score = text_match_score(&memory.content, &terms, query);
            (score, memory.confidence, memory.access_count, memory.id, memory)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.cmp(&a.3))
    });

    let top_score = scored.first().map(|entry| entry.0).unwrap_or(0);
    if top_score == 0 {
        return Vec::new();
    }
    let min_score = minimum_match_score(top_score);
    let mut selected: Vec<ConsolidatedMemory> = Vec::new();
    for (score, _confidence, _access_count, _id, memory) in scored {
        if score < min_score {
            continue;
        }
        let is_redundant = selected.iter().any(|existing| {
            text_similarity(&existing.content, &memory.content) >= 0.45
                && query_term_hits(&existing.content, &terms) == query_term_hits(&memory.content, &terms)
        });
        if is_redundant {
            continue;
        }
        selected.push(memory);
        if selected.len() >= limit {
            break;
        }
    }
    selected
}

fn focus_results<T, F>(items: Vec<T>, query: &str, limit: usize, text_fn: F) -> Vec<T>
where
    F: Fn(&T) -> &str,
{
    if items.is_empty() {
        return items;
    }

    let terms = query_terms(query);
    if terms.is_empty() {
        return items.into_iter().take(limit).collect();
    }

    let mut scored: Vec<(usize, T)> = items
        .into_iter()
        .map(|item| {
            let score = text_match_score(text_fn(&item), &terms, query);
            (score, item)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let top_score = scored.first().map(|entry| entry.0).unwrap_or(0);
    let min_score = minimum_match_score(top_score);
    let mut filtered: Vec<T> = scored
        .into_iter()
        .filter(|entry| entry.0 >= min_score)
        .map(|entry| entry.1)
        .take(limit)
        .collect();

    if filtered.is_empty() {
        filtered = Vec::new();
    }

    filtered
}

fn build_fts_query(query: &str) -> String {
    let mut fts_terms = Vec::new();
    for term in query_terms(query) {
        let mut saw_piece = false;
        for piece in split_compound_term(&term) {
            saw_piece = true;
            let piece = piece.to_string();
            if !fts_terms.contains(&piece) {
                fts_terms.push(piece);
            }
        }
        if !saw_piece && !fts_terms.contains(&term) {
            fts_terms.push(term);
        }
    }

    fts_terms
        .into_iter()
        .map(|term| format!("{}*", term))
        .collect::<Vec<_>>()
        .join(" OR ")
}
