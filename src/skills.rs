use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

use crate::db;

pub fn generate_skill_files(cons_conn: &Connection, skills_dir: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(skills_dir)?;
    let skills = db::get_all_skills(cons_conn)?;
    let mut written = Vec::new();

    for skill in &skills {
        let filename = format!("{}.md", skill.name);
        let path = skills_dir.join(&filename);
        let content = format_skill_markdown(&skill.name, &skill.content);
        std::fs::write(&path, content)?;
        written.push(filename);
    }

    Ok(written)
}

fn format_skill_markdown(name: &str, content: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Learned patterns for {name}\n---\n\n{content}\n"
    )
}
