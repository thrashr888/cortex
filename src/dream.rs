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
    raw_conn: &Connection,
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

    // Load graph data for analysis
    let entities = db::get_all_entities(raw_conn)?;
    let relationships = db::get_all_relationships(raw_conn)?;

    let entities_json = serde_json::to_string_pretty(
        &entities
            .iter()
            .map(|e| serde_json::json!({
                "id": e.id, "name": e.name, "type": e.entity_type,
                "description": e.description, "confidence": e.confidence, "access_count": e.access_count
            }))
            .collect::<Vec<_>>(),
    )?;

    let entity_names: std::collections::HashMap<i64, &str> = entities.iter().map(|e| (e.id, e.name.as_str())).collect();
    let relationships_json = serde_json::to_string_pretty(
        &relationships
            .iter()
            .map(|r| serde_json::json!({
                "source": entity_names.get(&r.source_entity_id).unwrap_or(&"?"),
                "target": entity_names.get(&r.target_entity_id).unwrap_or(&"?"),
                "type": r.relation_type, "weight": r.weight, "confidence": r.confidence
            }))
            .collect::<Vec<_>>(),
    )?;

    // Pass 1: Pattern mining with graph awareness
    let pattern_prompt = format!(
        r#"Analyze these consolidated memories and knowledge graph for cross-cutting patterns and insights.

Memories:
{cons_json}

Entities:
{entities_json}

Relationships:
{relationships_json}

Identify:
1. Recurring themes across multiple memories
2. Higher-order patterns (patterns of patterns)
3. Clusters of highly connected entities (conceptual groups)
4. Missing relationships (inferred from patterns)
5. Contradictory relationships
6. Potential blind spots or areas lacking coverage

Output JSON:
{{
  "consolidations": [
    {{"content": "description of insight", "type": "insight", "source_ids": [ids of related memories], "confidence": 0.0-1.0}}
  ],
  "skill_updates": [
    {{"name": "skill-name", "content": "comprehensive markdown skill file content"}}
  ],
  "new_entities": [
    {{"name": "EntityName", "type": "concept|pattern|technology", "description": "Short description"}}
  ],
  "new_relationships": [
    {{"source": "entity_name", "target": "entity_name", "type": "uses|implements|related_to", "confidence": 0.0-1.0}}
  ],
  "entity_updates": [
    {{"name": "entity_name", "description": "updated description", "confidence": 0.0-1.0}}
  ]
}}

Output ONLY valid JSON."#
    );

    let system = "You are a deep reflection system performing meta-analysis on learned knowledge and a knowledge graph. Output ONLY valid JSON.";
    let response = llm::call_anthropic(&pattern_prompt, system, config).await?;

    let json_str = extract_json(&response);
    let result: ConsolidationResult = serde_json::from_str(json_str)
        .unwrap_or_default();

    // Apply new entities from dream
    for entity in &result.new_entities {
        db::upsert_entity(raw_conn, &entity.name, &entity.r#type, entity.description.as_deref())?;
    }

    // Apply new relationships from dream
    for rel in &result.new_relationships {
        let source = db::get_entity_by_name(raw_conn, &rel.source)?;
        let target = db::get_entity_by_name(raw_conn, &rel.target)?;
        if let (Some(s), Some(t)) = (source, target) {
            db::upsert_relationship(raw_conn, s.id, t.id, &rel.r#type, 0, rel.confidence)?;
        }
    }

    // Apply entity updates
    for update in &result.entity_updates {
        db::update_entity(raw_conn, &update.name, update.description.as_deref(), update.confidence)?;
    }

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
