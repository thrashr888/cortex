use anyhow::Result;
use rusqlite::Connection;

use crate::db;
use crate::models::{ConsolidatedMemory, Skill, Stats};

pub fn format_context(
    cons_conn: &Connection,
    raw_conn: &Connection,
    global_cons_conn: Option<&Connection>,
    compact: bool,
) -> Result<String> {
    let consolidated = db::get_all_consolidated(cons_conn)?;
    let skills = db::get_all_skills(cons_conn)?;
    let stats = db::get_stats(raw_conn, cons_conn)?;

    let global_consolidated = match global_cons_conn {
        Some(gc) => db::get_all_consolidated(gc).unwrap_or_default(),
        None => vec![],
    };
    let global_skills = match global_cons_conn {
        Some(gc) => db::get_all_skills(gc).unwrap_or_default(),
        None => vec![],
    };

    if compact {
        Ok(format_compact(&consolidated, &skills, &stats, &global_consolidated))
    } else {
        Ok(format_full(&consolidated, &skills, &stats, &global_consolidated, &global_skills))
    }
}

fn format_full(
    consolidated: &[ConsolidatedMemory],
    skills: &[Skill],
    stats: &Stats,
    global_consolidated: &[ConsolidatedMemory],
    global_skills: &[Skill],
) -> String {
    let mut out = String::from("## Project Memory Context\n\n");

    if !consolidated.is_empty() {
        out.push_str("### Learned Patterns\n");
        for m in consolidated.iter().take(20) {
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
        for m in global_consolidated.iter().take(20) {
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
        "### Stats\n{} total memories | {} consolidated | {} skills\n",
        stats.raw_count, stats.consolidated_count, stats.skill_count
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
) -> String {
    let patterns: Vec<String> = consolidated
        .iter()
        .take(10)
        .map(|m| m.content.clone())
        .collect();

    let global_patterns: Vec<String> = global_consolidated
        .iter()
        .take(5)
        .map(|m| m.content.clone())
        .collect();

    let mut result = format!(
        "Project memory: {} memories, {} consolidated. Key patterns: {}",
        stats.raw_count,
        stats.consolidated_count,
        if patterns.is_empty() {
            "none yet".to_string()
        } else {
            patterns.join("; ")
        }
    );

    if !global_patterns.is_empty() {
        result.push_str(&format!(". Global: {}", global_patterns.join("; ")));
    }

    result
}
