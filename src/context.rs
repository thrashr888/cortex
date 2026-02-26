use anyhow::Result;
use rusqlite::Connection;

use crate::db;
use crate::models::{ConsolidatedMemory, Skill, Stats};

pub fn format_context(cons_conn: &Connection, raw_conn: &Connection, compact: bool) -> Result<String> {
    let consolidated = db::get_all_consolidated(cons_conn)?;
    let skills = db::get_all_skills(cons_conn)?;
    let stats = db::get_stats(raw_conn, cons_conn)?;

    if compact {
        Ok(format_compact(&consolidated, &skills, &stats))
    } else {
        Ok(format_full(&consolidated, &skills, &stats))
    }
}

fn format_full(consolidated: &[ConsolidatedMemory], skills: &[Skill], stats: &Stats) -> String {
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

    out.push_str(&format!(
        "### Stats\n{} total memories | {} consolidated | {} skills\n",
        stats.raw_count, stats.consolidated_count, stats.skill_count
    ));
    if let Some(ref last) = stats.last_sleep {
        out.push_str(&format!("Last consolidation: {}\n", last));
    }

    out
}

fn format_compact(consolidated: &[ConsolidatedMemory], _skills: &[Skill], stats: &Stats) -> String {
    let patterns: Vec<String> = consolidated
        .iter()
        .take(10)
        .map(|m| m.content.clone())
        .collect();

    format!(
        "Project memory: {} memories, {} consolidated. Key patterns: {}",
        stats.raw_count,
        stats.consolidated_count,
        if patterns.is_empty() {
            "none yet".to_string()
        } else {
            patterns.join("; ")
        }
    )
}
