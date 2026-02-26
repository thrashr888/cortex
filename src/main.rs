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
        /// Show global stats only
        #[arg(long)]
        global: bool,
    },
    /// Run memory consolidation
    Sleep {
        /// Micro sleep: SQL-only dedup and decay, no LLM call
        #[arg(long)]
        micro: bool,
        /// Quick sleep: LLM-powered consolidation (default)
        #[arg(long)]
        quick: bool,
        /// Operate on global ~/.cortex/ store
        #[arg(long, short)]
        global: bool,
    },
    /// Deep reflection: cross-session pattern mining
    Dream {
        /// Operate on global ~/.cortex/ store
        #[arg(long, short)]
        global: bool,
    },
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

/// Open global consolidated DB if ~/.cortex/ exists.
fn open_global_cons() -> Option<rusqlite::Connection> {
    init::find_global_dir().and_then(|gd| {
        db::open_consolidated_db(&gd.join("consolidated.db")).ok()
    })
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
            let mut memories = db::recall_memories(&raw_conn, &query, limit)?;

            // Also search global consolidated DB
            if let Some(global_cons) = open_global_cons() {
                let global_consolidated = db::get_all_consolidated(&global_cons).unwrap_or_default();
                let query_lower = query.to_lowercase();
                let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                for m in global_consolidated {
                    let content_lower = m.content.to_lowercase();
                    if query_words.iter().any(|w| content_lower.contains(w)) {
                        memories.push(models::Memory {
                            id: -m.id, // negative ID to distinguish global
                            content: format!("[global] {}", m.content),
                            r#type: m.r#type,
                            created_at: m.created_at,
                            accessed_at: m.updated_at,
                            access_count: m.access_count,
                            consolidated: true,
                            importance: m.confidence,
                            session_id: None,
                        });
                    }
                }
            }

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
        Commands::Stats { json, global } => {
            if global {
                let global_dir = init::find_global_dir()
                    .ok_or_else(|| anyhow::anyhow!("No global ~/.cortex/ directory found."))?;
                let global_cons = db::open_consolidated_db(&global_dir.join("consolidated.db"))?;
                let cons_count: i64 = global_cons.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0))?;
                let skill_count: i64 = global_cons.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))?;
                let last_sleep = db::get_meta(&global_cons, "last_sleep")?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                        "global_consolidated": cons_count,
                        "global_skills": skill_count,
                        "global_last_sleep": last_sleep,
                    }))?);
                } else {
                    println!("Global consolidated: {}", cons_count);
                    println!("Global skills: {}", skill_count);
                    if let Some(ref last) = last_sleep {
                        println!("Global last sleep: {}", last);
                    }
                }
            } else {
                let cortex_dir = find_cortex_dir(&cli.dir)?;
                let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
                let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
                let stats = db::get_stats(&raw_conn, &cons_conn)?;
                if json {
                    let mut stats_json = serde_json::to_value(&stats)?;
                    // Add global stats if available
                    if let Some(global_cons) = open_global_cons() {
                        let gc: i64 = global_cons.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0)).unwrap_or(0);
                        let gs: i64 = global_cons.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0)).unwrap_or(0);
                        stats_json["global_consolidated"] = serde_json::json!(gc);
                        stats_json["global_skills"] = serde_json::json!(gs);
                    }
                    println!("{}", serde_json::to_string_pretty(&stats_json)?);
                } else {
                    println!("{}", stats);
                    // Append global stats
                    if let Some(global_cons) = open_global_cons() {
                        let gc: i64 = global_cons.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0)).unwrap_or(0);
                        let gs: i64 = global_cons.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0)).unwrap_or(0);
                        if gc > 0 || gs > 0 {
                            println!("Global: {} consolidated, {} skills", gc, gs);
                        }
                    }
                }
            }
        }
        Commands::Sleep { micro, global, .. } => {
            if global {
                let global_dir = init::ensure_global_dir()?;
                let config = config::load_config(&global_dir)?;
                let raw_conn = db::open_raw_db(&global_dir.join("raw.db"))?;

                if micro {
                    let removed = sleep::micro_sleep(&raw_conn, &config)?;
                    eprintln!("Global micro sleep complete. Removed {} stale memories.", removed);
                } else {
                    let cons_conn = db::open_consolidated_db(&global_dir.join("consolidated.db"))?;
                    match sleep::quick_sleep(&raw_conn, &cons_conn, &config, &global_dir).await {
                        Ok(result) => {
                            eprintln!(
                                "Global quick sleep complete. {} consolidations, {} promotions, {} decayed, {} skills updated.",
                                result.consolidations.len(),
                                result.promotions.len(),
                                result.decayed.len(),
                                result.skill_updates.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("Global quick sleep failed: {}. Falling back to micro sleep.", e);
                            let removed = sleep::micro_sleep(&raw_conn, &config)?;
                            eprintln!("Global micro sleep complete. Removed {} stale memories.", removed);
                        }
                    }
                }
            } else {
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
        }
        Commands::Dream { global } => {
            if global {
                let global_dir = init::ensure_global_dir()?;
                let config = config::load_config(&global_dir)?;
                let raw_conn = db::open_raw_db(&global_dir.join("raw.db"))?;
                let cons_conn = db::open_consolidated_db(&global_dir.join("consolidated.db"))?;
                let result = dream::dream(&raw_conn, &cons_conn, &config, &global_dir).await?;
                eprintln!(
                    "Global dream complete. {} insights generated, {} skills updated.",
                    result.insights, result.skills_updated
                );
            } else {
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
        }
        Commands::Wake => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let config = config::load_config(&cortex_dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let global_cons = open_global_cons();
            let ctx = wake::wake(&raw_conn, &cons_conn, &config, &cortex_dir, global_cons.as_ref()).await?;
            println!("{}", ctx);
        }
        Commands::Context { compact } => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let global_cons = open_global_cons();
            let ctx = context::format_context(&cons_conn, &raw_conn, global_cons.as_ref(), compact)?;
            println!("{}", ctx);
        }
        Commands::Mcp => {
            let cortex_dir = find_cortex_dir(&cli.dir)?;
            let sid = session_id();
            let global_dir = init::find_global_dir();
            mcp::run_mcp_server(cortex_dir, sid, global_dir).await?;
        }
    }

    Ok(())
}
