use anyhow::Result;
use rusqlite::Connection;

use crate::config::Config;
use crate::context;
use crate::db;
use crate::sleep;

pub async fn wake(
    raw_conn: &Connection,
    cons_conn: &Connection,
    config: &Config,
    cortex_dir: &std::path::Path,
    global_cons_conn: Option<&Connection>,
) -> Result<String> {
    let uncons = db::get_unconsolidated_count(raw_conn)?;

    if uncons > 0 {
        eprintln!("Found {} unconsolidated memories, running catch-up...", uncons);
        // Try quick sleep, fall back to micro if no API key
        match sleep::quick_sleep(raw_conn, cons_conn, config, cortex_dir).await {
            Ok(_) => eprintln!("Catch-up consolidation complete."),
            Err(e) => {
                eprintln!("Quick sleep failed ({}), running micro sleep...", e);
                sleep::micro_sleep(raw_conn, config)?;
            }
        }
    }

    context::format_context(cons_conn, raw_conn, global_cons_conn, false)
}
