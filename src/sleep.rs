use anyhow::Result;
use rusqlite::Connection;

use crate::config;
use crate::config::Config;
use crate::db;
use crate::dream;
use crate::init;
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

    // Apply global promotions to ~/.cortex/
    if !result.global_promotions.is_empty() {
        match init::ensure_global_dir() {
            Ok(global_dir) => {
                let global_cons = db::open_consolidated_db(&global_dir.join("consolidated.db"))?;
                let mut promoted = 0;
                for gp in &result.global_promotions {
                    // Skip duplicates
                    if db::consolidated_content_exists(&global_cons, &gp.content)? {
                        continue;
                    }
                    db::insert_consolidated(&global_cons, &gp.content, &gp.r#type, &[], gp.confidence)?;
                    promoted += 1;
                }
                if promoted > 0 {
                    skills::generate_skill_files(&global_cons, &global_dir.join("skills"))?;
                    db::set_meta(&global_cons, "last_sleep", &chrono::Utc::now().to_rfc3339())?;
                    eprintln!("Promoted {} new memories to global store.", promoted);
                }

                // Auto global dream: if 5+ entries and last dream was 7+ days ago (or never)
                auto_global_dream(&global_dir, &global_cons).await;
            }
            Err(e) => {
                eprintln!("Warning: could not write global promotions: {}", e);
            }
        }
    }

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
- "global_promotions": array of {{"content": "description", "type": "preference|pattern", "confidence": 0.0-1.0}}
  Identify user-level knowledge that applies across ALL projects: personal identity (name, role),
  tool preferences, coding style, workflow habits, language preferences. NOT project-specific patterns.

Rules:
- Merge similar observations into single consolidated patterns
- Detect contradictions between old and new knowledge
- Promote unique high-value observations directly
- Decay superseded long-term memories
- Generate skill files for recurring patterns (3+ related observations)
- Put cross-project personal preferences and identity in global_promotions, not consolidations
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

/// Auto-trigger global dream if enough entries exist and it hasn't been done recently.
async fn auto_global_dream(global_dir: &std::path::Path, global_cons: &rusqlite::Connection) {
    let count = db::get_consolidated_count(global_cons).unwrap_or(0);
    if count < 5 {
        return;
    }

    let should_dream = match db::get_meta(global_cons, "last_dream") {
        Ok(Some(last)) => {
            if let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(&last) {
                let last_utc = last_dt.with_timezone(&chrono::Utc);
                let days = chrono::Utc::now().signed_duration_since(last_utc).num_days();
                days >= 1
            } else {
                true
            }
        }
        _ => true, // never dreamed
    };

    if !should_dream {
        return;
    }

    eprintln!("Auto-running global dream ({} entries, overdue)...", count);
    let global_config = config::load_config(global_dir).unwrap_or_default();
    let global_raw = match db::open_raw_db(&global_dir.join("raw.db")) {
        Ok(c) => c,
        Err(_) => return,
    };
    match dream::dream(&global_raw, global_cons, &global_config, global_dir).await {
        Ok(result) => {
            eprintln!(
                "Global dream complete. {} insights, {} skills updated.",
                result.insights, result.skills_updated
            );
        }
        Err(e) => {
            eprintln!("Global dream failed: {}", e);
        }
    }
}

impl Default for ConsolidationResult {
    fn default() -> Self {
        Self {
            consolidations: vec![],
            contradictions: vec![],
            promotions: vec![],
            decayed: vec![],
            skill_updates: vec![],
            global_promotions: vec![],
        }
    }
}
