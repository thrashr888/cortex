use anyhow::Result;
use rusqlite::Connection;

use crate::db;
use crate::models::{ConsolidatedMemory, Entity, Relationship, Skill, Stats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeDecision {
    KeepBoth,
    ProjectOnly,
    GlobalOnly,
}

pub fn scope_decision_for_query(
    project_top: Option<(&str, &str)>,
    global_top: Option<(&str, &str)>,
    query: &str,
) -> ScopeDecision {
    if query.trim().is_empty() {
        return ScopeDecision::KeepBoth;
    }

    let scoring_terms = scoring_terms(query);
    let query_term_count = scoring_terms.len();
    let project_strength = project_top
        .map(|(content, _)| query_term_hits(content, &scoring_terms))
        .unwrap_or(0);
    let global_strength = global_top
        .map(|(content, _)| query_term_hits(content, &scoring_terms))
        .unwrap_or(0);
    let project_score = project_top
        .map(|(content, _)| text_match_score(content, &scoring_terms, query))
        .unwrap_or(0);
    let global_score = global_top
        .map(|(content, _)| text_match_score(content, &scoring_terms, query))
        .unwrap_or(0);
    let preference_query = query_terms(query).iter().any(|term| is_routing_term(term));
    let project_is_preference = project_top
        .map(|(_, memory_type)| memory_type == "preference")
        .unwrap_or(false);
    let global_is_preference = global_top
        .map(|(_, memory_type)| memory_type == "preference")
        .unwrap_or(false);

    if preference_query && global_is_preference && !project_is_preference && global_strength > 0 {
        ScopeDecision::GlobalOnly
    } else if global_score > project_score && project_strength < query_term_count {
        ScopeDecision::GlobalOnly
    } else if project_strength >= query_term_count && project_score > global_score {
        ScopeDecision::ProjectOnly
    } else {
        ScopeDecision::KeepBoth
    }
}

pub fn format_context(
    cons_conn: &Connection,
    raw_conn: &Connection,
    global_cons_conn: Option<&Connection>,
    compact: bool,
    query: Option<&str>,
    limit: usize,
) -> Result<String> {
    // Load memories - either search-based (relevant) or all
    let mut consolidated = match query {
        Some(q) if !q.trim().is_empty() => db::search_consolidated(cons_conn, q, limit)?,
        _ => {
            // No query: load top N by recency
            let all = db::get_all_consolidated(cons_conn)?;
            all.into_iter().take(limit).collect()
        }
    };

    let skills = db::get_all_skills(cons_conn)?;
    let stats = db::get_stats(raw_conn, cons_conn)?;

    // Also apply query filter to global memories
    let mut global_consolidated = match global_cons_conn {
        Some(gc) => match query {
            Some(q) if !q.trim().is_empty() => {
                db::search_consolidated(gc, q, std::cmp::max(1, limit / 2)).unwrap_or_default()
            },
            _ => {
                let all = db::get_all_consolidated(gc).unwrap_or_default();
                all.into_iter().take(limit / 3).collect()
            }
        },
        None => vec![],
    };
    let global_skills = match global_cons_conn {
        Some(gc) => db::get_all_skills(gc).unwrap_or_default(),
        None => vec![],
    };

    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        match scope_decision_for_query(
            consolidated.first().map(|m| (m.content.as_str(), m.r#type.as_str())),
            global_consolidated
                .first()
                .map(|m| (m.content.as_str(), m.r#type.as_str())),
            q,
        ) {
            ScopeDecision::ProjectOnly => global_consolidated.clear(),
            ScopeDecision::GlobalOnly => consolidated.clear(),
            ScopeDecision::KeepBoth => {}
        }
    }

    // Load entities - either query-relevant or top by access
    let entities = if consolidated.is_empty() && query.is_some() {
        vec![]
    } else {
        match query {
            Some(q) if !q.trim().is_empty() => db::search_entities(raw_conn, q, limit)?,
            _ => {
                let all = db::get_all_entities(raw_conn)?;
                all.into_iter().take(limit).collect()
            }
        }
    };

    // Load relationships for displayed entities
    let entity_ids: Vec<i64> = entities.iter().map(|e| e.id).collect();
    let relationships = if !entity_ids.is_empty() {
        let all_rels = db::get_all_relationships(raw_conn)?;
        all_rels
            .into_iter()
            .filter(|r| entity_ids.contains(&r.source_entity_id) || entity_ids.contains(&r.target_entity_id))
            .collect()
    } else {
        vec![]
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

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for word in query.split_whitespace() {
        let normalized = normalize_query_term(
            &word
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
                .to_lowercase(),
        );
        push_query_term(&mut terms, normalized.clone());
        for piece in split_compound_term(&normalized) {
            push_query_term(&mut terms, normalize_query_term(piece));
        }
    }
    terms
}

fn split_compound_term(term: &str) -> impl Iterator<Item = &str> {
    term.split(|c| c == '-' || c == '_').filter(|piece| !piece.is_empty())
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
        .filter(|term| !is_routing_term(term) && !term.contains('-') && !term.contains('_'))
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
    let phrase_bonus = if !full_query.trim().is_empty() && lower.contains(&full_query.to_lowercase()) {
        1
    } else {
        0
    };
    term_hits * 10 + phrase_bonus
}

fn query_term_hits(text: &str, terms: &[String]) -> usize {
    let lower = text.to_lowercase();
    terms.iter().filter(|term| lower.contains(term.as_str())).count()
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
