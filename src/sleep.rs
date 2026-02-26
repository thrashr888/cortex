use anyhow::Result;
use rusqlite::Connection;

use crate::config::Config;
use crate::db;
use crate::llm;
use crate::models::ConsolidationResult;
use crate::skills;

/// Micro sleep: pure SQL operations, no LLM call.
/// Dedup exact matches, update decay scores, delete below threshold.
pub fn micro_sleep(raw_conn: &Connection, config: &Config) -> Result<u64> {
    let mut removed = 0u64;

    // Dedup exact content matches (keep the one with highest access_count)
    let dupes: Vec<i64> = {
        let mut stmt = raw_conn.prepare(
            "SELECT m1.id FROM memories m1
             INNER JOIN memories m2 ON m1.content = m2.content AND m1.id < m2.id
             WHERE m1.consolidated = 0 AND m2.consolidated = 0",
        )?;
        let rows: Vec<i64> = stmt.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };
    for id in &dupes {
        db::delete_memory(raw_conn, *id)?;
        removed += 1;
    }

    // Decay: compute score = importance * (access_count + 1) / (days_since_access + 1)
    // Delete memories below threshold that are already consolidated
    let threshold = config.consolidation.decay_threshold;
    let decayed: Vec<i64> = {
        let mut stmt = raw_conn.prepare(
            "SELECT id FROM memories
             WHERE consolidated = 1
             AND (importance * (access_count + 1.0) / (julianday('now') - julianday(accessed_at) + 1.0)) < ?1",
        )?;
        let rows: Vec<i64> = stmt.query_map(rusqlite::params![threshold], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };
    for id in &decayed {
        db::delete_memory(raw_conn, *id)?;
        removed += 1;
    }

    Ok(removed)
}

/// Quick sleep: gather unprocessed memories, call LLM for consolidation, apply results.
pub async fn quick_sleep(
    raw_conn: &Connection,
    cons_conn: &Connection,
    config: &Config,
    cortex_dir: &std::path::Path,
) -> Result<ConsolidationResult> {
    let unprocessed = db::get_unconsolidated_memories(raw_conn)?;
    if unprocessed.is_empty() {
        return Ok(ConsolidationResult::default());
    }

    let existing = db::get_all_consolidated(cons_conn)?;
    let prompt = build_consolidation_prompt(&unprocessed, &existing);

    let system = "You are a memory consolidation system. Analyze observations and output ONLY valid JSON.";
    let response = llm::call_anthropic(&prompt, system, config).await?;

    // Extract JSON from response (handle markdown code blocks)
    let json_str = extract_json(&response);
    let result: ConsolidationResult = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse consolidation JSON: {}. Response: {}", e, &response))?;

    apply_consolidation(raw_conn, cons_conn, &result, &unprocessed)?;

    // Update skill files
    skills::generate_skill_files(cons_conn, &cortex_dir.join("skills"))?;

    // Record sleep time
    db::set_meta(cons_conn, "last_sleep", &chrono::Utc::now().to_rfc3339())?;

    Ok(result)
}

fn build_consolidation_prompt(
    unprocessed: &[crate::models::Memory],
    existing: &[crate::models::ConsolidatedMemory],
) -> String {
    let recent_json = serde_json::to_string_pretty(
        &unprocessed
            .iter()
            .map(|m| serde_json::json!({"id": m.id, "content": m.content, "type": m.r#type, "created_at": m.created_at}))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();

    let existing_json = serde_json::to_string_pretty(
        &existing
            .iter()
            .map(|m| serde_json::json!({"id": m.id, "content": m.content, "type": m.r#type, "confidence": m.confidence}))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();

    format!(
        r#"Given these recent observations and existing long-term memories, consolidate them.

Recent observations (unprocessed):
{recent_json}

Existing long-term memories:
{existing_json}

Output a JSON object with these fields:
- "consolidations": array of {{"content": "merged abstract pattern", "type": "pattern|bugfix|decision|preference", "source_ids": [list of recent observation ids merged], "confidence": 0.0-1.0}}
- "contradictions": array of {{"old_id": existing_memory_id, "new_id": recent_observation_id, "resolution": "keep_new|keep_old|merge"}}
- "promotions": array of recent observation IDs that should be promoted to long-term as-is (high value, unique)
- "decayed": array of existing long-term memory IDs that are superseded or no longer relevant
- "skill_updates": array of {{"name": "skill-name-kebab-case", "content": "markdown content describing the learned skill/pattern"}}

Rules:
- Merge similar observations into single consolidated patterns
- Detect contradictions between old and new knowledge
- Promote unique high-value observations directly
- Decay superseded long-term memories
- Generate skill files for recurring patterns (3+ related observations)
- Output ONLY valid JSON, no explanation"#
    )
}

fn apply_consolidation(
    raw_conn: &Connection,
    cons_conn: &Connection,
    result: &ConsolidationResult,
    unprocessed: &[crate::models::Memory],
) -> Result<()> {
    // Apply consolidations
    for c in &result.consolidations {
        db::insert_consolidated(cons_conn, &c.content, &c.r#type, &c.source_ids, c.confidence)?;
    }

    // Apply promotions (copy raw memory to consolidated)
    for raw_id in &result.promotions {
        if let Some(m) = unprocessed.iter().find(|m| m.id == *raw_id) {
            db::insert_consolidated(cons_conn, &m.content, &m.r#type, &[m.id], m.importance)?;
        }
    }

    // Apply decayed (remove from consolidated)
    db::remove_consolidated(cons_conn, &result.decayed)?;

    // Apply skill updates
    for su in &result.skill_updates {
        db::upsert_skill(cons_conn, &su.name, &su.content, &[])?;
    }

    // Mark all unprocessed as consolidated
    let ids: Vec<i64> = unprocessed.iter().map(|m| m.id).collect();
    db::mark_consolidated(raw_conn, &ids)?;

    Ok(())
}

fn extract_json(text: &str) -> &str {
    // Try to find JSON in markdown code blocks.
    // Use rfind for the closing fence since skill content may contain nested code blocks.
    if let Some(start) = text.find("```json") {
        let content = &text[start + 7..];
        if let Some(end) = content.rfind("```") {
            return content[..end].trim();
        }
    }
    if let Some(start) = text.find("```") {
        let content = &text[start + 3..];
        if let Some(end) = content.rfind("```") {
            return content[..end].trim();
        }
    }
    // Try to find a JSON object directly
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].trim();
        }
    }
    text.trim()
}

impl Default for ConsolidationResult {
    fn default() -> Self {
        Self {
            consolidations: vec![],
            contradictions: vec![],
            promotions: vec![],
            decayed: vec![],
            skill_updates: vec![],
        }
    }
}
