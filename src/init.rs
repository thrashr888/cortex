use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::db;

/// Initialize a cortex directory with DBs and config.
/// Shared between project init and global init.
fn init_cortex_dir(cortex_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(cortex_dir.join("skills"))?;

    // Initialize databases
    let _raw = db::open_raw_db(&cortex_dir.join("raw.db"))?;
    let _cons = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;

    // Write default config
    let config = Config::default();
    let config_str = toml::to_string_pretty(&config)?;
    std::fs::write(cortex_dir.join("config.toml"), config_str)?;

    Ok(())
}

pub fn init_cortex(base_dir: &Path) -> Result<()> {
    let cortex_dir = base_dir.join(".cortex");
    if cortex_dir.exists() {
        eprintln!(".cortex/ already initialized in {}", base_dir.display());
        return Ok(());
    }

    init_cortex_dir(&cortex_dir)?;

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

/// Return the global cortex directory path if it exists.
pub fn find_global_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let global_dir = home.join(".cortex");
    if global_dir.exists() {
        Some(global_dir)
    } else {
        None
    }
}

/// Ensure the global cortex directory exists, creating it if needed.
pub fn ensure_global_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    let global_dir = home.join(".cortex");
    if !global_dir.exists() {
        init_cortex_dir(&global_dir)?;
        eprintln!("Initialized global ~/.cortex/");
    }
    Ok(global_dir)
}
