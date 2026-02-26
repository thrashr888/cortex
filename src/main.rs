mod config;
mod context;
mod db;
mod dream;
mod init;
mod llm;
mod mcp;
mod models;
mod skills;
mod sleep;
mod wake;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cortex", about = "Repo-local cognitive memory for AI agents")]
struct Cli {
    /// Path to the project root (defaults to current directory)
    #[arg(long, global = true)]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize .cortex/ in the current directory
    Init,
    /// Save a learning, decision, or pattern
    Save {
        /// What was learned or observed
        content: String,
        /// Type: bugfix, decision, pattern, preference, observation
        #[arg(long, default_value = "observation")]
        r#type: String,
    },
    /// Search project memory
    Recall {
        /// Search query
        query: String,
        /// Max results
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Memory health statistics
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run memory consolidation
    Sleep {
        /// Micro sleep: SQL-only dedup and decay, no LLM call
        #[arg(long)]
        micro: bool,
        /// Quick sleep: LLM-powered consolidation (default)
        #[arg(long)]
        quick: bool,
    },
    /// Deep reflection: cross-session pattern mining
    Dream,
    /// Session start: catch-up consolidation and context injection
    Wake,
    /// Output memory context for prompt injection
    Context {
        /// Compact single-line format
        #[arg(long)]
        compact: bool,
    },
    /// Start MCP stdio server
    Mcp,
}

fn find_cortex_dir(base: &Option<PathBuf>) -> Result<PathBuf> {
    let base = match base {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    let cortex_dir = base.join(".cortex");
    if !cortex_dir.exists() {
        anyhow::bail!(
            "No .cortex/ directory found in {}. Run `cortex init` first.",
            base.display()
        );
    }
    Ok(cortex_dir)
}

fn session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let base = cli.dir.unwrap_or(std::env::current_dir()?);
            init::init_cortex(&base)?;
        }
        Commands::Save { content, r#type } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let config = config::load_config(&cortex_dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let sid = session_id();
            let id = db::save_memory(&raw_conn, &content, &r#type, &sid)?;
            eprintln!("Saved memory #{} (type: {})", id, r#type);

            // Auto micro-sleep
            let uncons = db::get_unconsolidated_count(&raw_conn)?;
            if uncons >= config.consolidation.auto_micro_threshold as i64 {
                let removed = sleep::micro_sleep(&raw_conn, &config)?;
                if removed > 0 {
                    eprintln!("Auto micro-sleep: removed {} stale memories", removed);
                }
            }
        }
        Commands::Recall { query, limit, json } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let memories = db::recall_memories(&raw_conn, &query, limit)?;
            if memories.is_empty() {
                eprintln!("No memories found.");
            } else if json {
                println!("{}", serde_json::to_string_pretty(&memories)?);
            } else {
                for m in &memories {
                    println!("[{}] #{}: {}", m.r#type, m.id, m.content);
                }
            }
        }
        Commands::Stats { json } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let stats = db::get_stats(&raw_conn, &cons_conn)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                println!("{}", stats);
            }
        }
        Commands::Sleep { micro, .. } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let config = config::load_config(&cortex_dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;

            if micro {
                let removed = sleep::micro_sleep(&raw_conn, &config)?;
                eprintln!("Micro sleep complete. Removed {} stale memories.", removed);
            } else {
                let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
                match sleep::quick_sleep(&raw_conn, &cons_conn, &config, &cortex_dir).await {
                    Ok(result) => {
                        eprintln!(
                            "Quick sleep complete. {} consolidations, {} promotions, {} decayed, {} skills updated.",
                            result.consolidations.len(),
                            result.promotions.len(),
                            result.decayed.len(),
                            result.skill_updates.len()
                        );
                    }
                    Err(e) => {
                        eprintln!("Quick sleep failed: {}. Falling back to micro sleep.", e);
                        let removed = sleep::micro_sleep(&raw_conn, &config)?;
                        eprintln!("Micro sleep complete. Removed {} stale memories.", removed);
                    }
                }
            }
        }
        Commands::Dream => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let config = config::load_config(&cortex_dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let result = dream::dream(&raw_conn, &cons_conn, &config, &cortex_dir).await?;
            eprintln!(
                "Dream complete. {} insights generated, {} skills updated.",
                result.insights, result.skills_updated
            );
        }
        Commands::Wake => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let config = config::load_config(&cortex_dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let ctx = wake::wake(&raw_conn, &cons_conn, &config, &cortex_dir).await?;
            println!("{}", ctx);
        }
        Commands::Context { compact } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let ctx = context::format_context(&cons_conn, &raw_conn, compact)?;
            println!("{}", ctx);
        }
        Commands::Mcp => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let sid = session_id();
            mcp::run_mcp_server(cortex_dir, sid).await?;
        }
    }

    Ok(())
}
