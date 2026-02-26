use anyhow::Result;
use rusqlite::Connection;

use crate::config::Config;
use crate::db;
use crate::llm;
use crate::models::ConsolidationResult;
use crate::skills;

/// Deep reflection: cross-session pattern mining and meta-learning.
/// Runs 2-3 LLM calls for comprehensive analysis.
pub async fn dream(
    _raw_conn: &Connection,
    cons_conn: &Connection,
    config: &Config,
    cortex_dir: &std::path::Path,
) -> Result<DreamResult> {
    let consolidated = db::get_all_consolidated(cons_conn)?;
    if consolidated.is_empty() {
        return Ok(DreamResult { insights: 0, skills_updated: 0 });
    }

    let cons_json = serde_json::to_string_pretty(
        &consolidated
            .iter()
            .map(|m| serde_json::json!({
                "id": m.id, "content": m.content, "type": m.r#type,
                "confidence": m.confidence, "access_count": m.access_count
            }))
            .collect::<Vec<_>>(),
    )?;

    // Pass 1: Pattern mining
    let pattern_prompt = format!(
        r#"Analyze these consolidated memories for cross-cutting patterns and insights.

Memories:
{cons_json}

Identify:
1. Recurring themes across multiple memories
2. Higher-order patterns (patterns of patterns)
3. Potential blind spots or areas lacking coverage

Output JSON:
{{
  "insights": [
    {{"content": "description of insight", "type": "insight", "source_ids": [ids of related memories], "confidence": 0.0-1.0}}
  ],
  "skill_updates": [
    {{"name": "skill-name", "content": "comprehensive markdown skill file content"}}
  ]
}}

Output ONLY valid JSON."#
    );

    let system = "You are a deep reflection system performing meta-analysis on learned knowledge. Output ONLY valid JSON.";
    let response = llm::call_anthropic(&pattern_prompt, system, config).await?;

    let json_str = extract_json(&response);
    let result: ConsolidationResult = serde_json::from_str(json_str)
        .unwrap_or_default();

    // Apply insights as new consolidated memories
    let mut insights = 0;
    for c in &result.consolidations {
        db::insert_consolidated(cons_conn, &c.content, "insight", &c.source_ids, c.confidence)?;
        insights += 1;
    }

    // Apply skill updates
    let mut skills_updated = 0;
    for su in &result.skill_updates {
        db::upsert_skill(cons_conn, &su.name, &su.content, &[])?;
        skills_updated += 1;
    }

    // Regenerate all skill files
    skills::generate_skill_files(cons_conn, &cortex_dir.join("skills"))?;

    // Record dream time
    db::set_meta(cons_conn, "last_dream", &chrono::Utc::now().to_rfc3339())?;
    db::set_meta(cons_conn, "last_sleep", &chrono::Utc::now().to_rfc3339())?;

    Ok(DreamResult { insights, skills_updated })
}

pub struct DreamResult {
    pub insights: usize,
    pub skills_updated: usize,
}

fn extract_json(text: &str) -> &str {
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
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].trim();
        }
    }
    text.trim()
}
