use anyhow::Result;
use rusqlite::Connection;

use crate::db;
use crate::models::{ConsolidatedMemory, Entity, Relationship, Skill, Stats};

pub fn format_context(
    cons_conn: &Connection,
    raw_conn: &Connection,
    global_cons_conn: Option<&Connection>,
    compact: bool,
    query: Option<&str>,
    limit: usize,
) -> Result<String> {
    // Load memories - either search-based (relevant) or all
    let consolidated = match query {
        Some(q) if !q.trim().is_empty() => db::search_consolidated(cons_conn, q, limit)?,
        _ => {
            // No query: load top N by recency
            let all = db::get_all_consolidated(cons_conn)?;
            all.into_iter().take(limit).collect()
        }
    };

    let skills = db::get_all_skills(cons_conn)?;
    let stats = db::get_stats(raw_conn, cons_conn)?;

    // Load entities - either query-relevant or top by access
    let entities = match query {
        Some(q) if !q.trim().is_empty() => db::search_entities(raw_conn, q, limit)?,
        _ => {
            let all = db::get_all_entities(raw_conn)?;
            all.into_iter().take(limit).collect()
        }
    };

    // Load relationships for displayed entities
    let entity_ids: Vec<i64> = entities.iter().map(|e| e.id).collect();
    let relationships = if !entity_ids.is_empty() {
        let all_rels = db::get_all_relationships(raw_conn)?;
        all_rels.into_iter().filter(|r| {
            entity_ids.contains(&r.source_entity_id) || entity_ids.contains(&r.target_entity_id)
        }).collect()
    } else {
        vec![]
    };

    // Also apply query filter to global memories
    let global_consolidated = match global_cons_conn {
        Some(gc) => {
            match query {
                Some(q) if !q.trim().is_empty() => db::search_consolidated(gc, q, limit / 2).unwrap_or_default(),
                _ => {
                    let all = db::get_all_consolidated(gc).unwrap_or_default();
                    all.into_iter().take(limit / 3).collect()
                }
            }
        }
        None => vec![],
    };
    let global_skills = match global_cons_conn {
        Some(gc) => db::get_all_skills(gc).unwrap_or_default(),
        None => vec![],
    };

    if compact {
        Ok(format_compact(&consolidated, &skills, &stats, &global_consolidated, &entities))
    } else {
        Ok(format_full(&consolidated, &skills, &stats, &global_consolidated, &global_skills, &entities, &relationships))
    }
}

fn format_full(
    consolidated: &[ConsolidatedMemory],
    skills: &[Skill],
    stats: &Stats,
    global_consolidated: &[ConsolidatedMemory],
    global_skills: &[Skill],
    entities: &[Entity],
    relationships: &[Relationship],
) -> String {
    let mut out = String::from("## Project Memory Context\n\n");

    // Entity section
    if !entities.is_empty() {
        out.push_str("### Key Entities\n");
        let entity_map: std::collections::HashMap<i64, &Entity> = entities.iter().map(|e| (e.id, e)).collect();
        for e in entities {
            let desc = e.description.as_deref().unwrap_or("");
            out.push_str(&format!(
                "- **{}** ({}, confidence: {:.2}): {}\n",
                e.name, e.entity_type, e.confidence, desc
            ));
            // Show relationships for this entity
            let rels: Vec<&Relationship> = relationships.iter()
                .filter(|r| r.source_entity_id == e.id || r.target_entity_id == e.id)
                .collect();
            for r in rels {
                let other_id = if r.source_entity_id == e.id { r.target_entity_id } else { r.source_entity_id };
                if let Some(other) = entity_map.get(&other_id) {
                    if r.source_entity_id == e.id {
                        out.push_str(&format!("  - {} {}\n", r.relation_type, other.name));
                    } else {
                        out.push_str(&format!("  - {} by {}\n", r.relation_type, other.name));
                    }
                }
            }
        }
        out.push('\n');
    }

    if !consolidated.is_empty() {
        out.push_str("### Learned Patterns\n");
        for m in consolidated {
            out.push_str(&format!(
                "- [{}] {} (confidence: {:.2})\n",
                m.r#type, m.content, m.confidence
            ));
        }
        out.push('\n');
    }

    if !skills.is_empty() {
        out.push_str("### Skills\n");
        for s in skills {
            let line_count = s.content.lines().count();
            out.push_str(&format!("- {}: {} lines\n", s.name, line_count));
        }
        out.push('\n');
    }

    if !global_consolidated.is_empty() {
        out.push_str("### Global Knowledge\n");
        for m in global_consolidated {
            out.push_str(&format!(
                "- [{}] {} (confidence: {:.2})\n",
                m.r#type, m.content, m.confidence
            ));
        }
        out.push('\n');
    }

    if !global_skills.is_empty() {
        out.push_str("### Global Skills\n");
        for s in global_skills {
            let line_count = s.content.lines().count();
            out.push_str(&format!("- {}: {} lines\n", s.name, line_count));
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "### Stats\n{} total memories | {} consolidated | {} entities | {} skills\n",
        stats.raw_count, stats.consolidated_count, stats.entity_count, stats.skill_count
    ));
    if !global_consolidated.is_empty() {
        out.push_str(&format!("{} global patterns\n", global_consolidated.len()));
    }
    if let Some(ref last) = stats.last_sleep {
        out.push_str(&format!("Last consolidation: {}\n", last));
    }

    out
}

fn format_compact(
    consolidated: &[ConsolidatedMemory],
    _skills: &[Skill],
    stats: &Stats,
    global_consolidated: &[ConsolidatedMemory],
    entities: &[Entity],
) -> String {
    let patterns: Vec<String> = consolidated
        .iter()
        .map(|m| m.content.clone())
        .collect();

    let global_patterns: Vec<String> = global_consolidated
        .iter()
        .map(|m| m.content.clone())
        .collect();

    let entity_names: Vec<String> = entities
        .iter()
        .take(10)
        .map(|e| e.name.clone())
        .collect();

    let mut result = format!(
        "Project memory: {} memories, {} consolidated, {} entities. Key patterns: {}",
        stats.raw_count,
        stats.consolidated_count,
        stats.entity_count,
        if patterns.is_empty() {
            "none yet".to_string()
        } else {
            patterns.join("; ")
        }
    );

    if !entity_names.is_empty() {
        result.push_str(&format!(". Entities: {}", entity_names.join(", ")));
    }

    if !global_patterns.is_empty() {
        result.push_str(&format!(". Global: {}", global_patterns.join("; ")));
    }

    result
}
