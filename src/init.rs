use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::db;

pub fn init_cortex(base_dir: &Path) -> Result<()> {
    let cortex_dir = base_dir.join(".cortex");
    if cortex_dir.exists() {
        anyhow::bail!(".cortex/ already exists in {}", base_dir.display());
    }

    std::fs::create_dir_all(cortex_dir.join("skills"))?;

    // Initialize databases
    let _raw = db::open_raw_db(&cortex_dir.join("raw.db"))?;
    let _cons = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;

    // Write default config
    let config = Config::default();
    let config_str = toml::to_string_pretty(&config)?;
    std::fs::write(cortex_dir.join("config.toml"), config_str)?;

    // Append to .gitignore if it exists
    let gitignore = base_dir.join(".gitignore");
    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore)?;
        if !content.contains(".cortex/raw.db") {
            let mut append = String::new();
            if !content.ends_with('\n') {
                append.push('\n');
            }
            append.push_str(".cortex/raw.db\n.cortex/raw.db-wal\n.cortex/raw.db-shm\n");
            std::fs::write(&gitignore, format!("{}{}", content, append))?;
        }
    }

    eprintln!("Initialized .cortex/ in {}", base_dir.display());
    Ok(())
}
