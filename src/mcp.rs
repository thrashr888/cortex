use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::config;
use crate::context;
use crate::db;
use crate::sleep;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub async fn run_mcp_server(cortex_dir: PathBuf, session_id: String) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError { code: -32700, message: e.to_string() }),
                };
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(Value::Null);
        let result = handle_request(&req, &cortex_dir, &session_id).await;

        let resp = match result {
            Ok(val) => JsonRpcResponse { jsonrpc: "2.0".into(), id, result: Some(val), error: None },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError { code: -32603, message: e.to_string() }),
            },
        };

        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
        stdout.flush()?;
    }

    Ok(())
}

async fn handle_request(req: &JsonRpcRequest, cortex_dir: &PathBuf, session_id: &str) -> Result<Value> {
    match req.method.as_str() {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "cortex",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "tools/list" => Ok(serde_json::json!({
            "tools": [
                {
                    "name": "cortex_save",
                    "description": "Save a learning, decision, or pattern to project memory",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "What was learned or observed" },
                            "type": { "type": "string", "description": "Type: bugfix, decision, pattern, preference, observation", "default": "observation" }
                        },
                        "required": ["content"]
                    }
                },
                {
                    "name": "cortex_recall",
                    "description": "Search project memory for relevant learnings",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search query" },
                            "limit": { "type": "integer", "description": "Max results (default 10)" }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "cortex_context",
                    "description": "Get current memory context for injection into agent prompts",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "compact": { "type": "boolean", "description": "Return compact single-line format", "default": false }
                        }
                    }
                },
                {
                    "name": "cortex_sleep",
                    "description": "Run memory consolidation. Use micro=true for fast SQL-only mode.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "micro": { "type": "boolean", "description": "Use micro sleep (SQL-only, no LLM call)", "default": false }
                        }
                    }
                },
                {
                    "name": "cortex_stats",
                    "description": "Get memory health statistics",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        })),
        "tools/call" => {
            let tool_name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            let text = call_tool(tool_name, &args, cortex_dir, session_id).await?;
            Ok(serde_json::json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        _ => anyhow::bail!("Unknown method: {}", req.method),
    }
}

async fn call_tool(name: &str, args: &Value, cortex_dir: &PathBuf, session_id: &str) -> Result<String> {
    match name {
        "cortex_save" => {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let mem_type = args.get("type").and_then(|v| v.as_str()).unwrap_or("observation");
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let config = config::load_config(cortex_dir)?;
            let id = db::save_memory(&raw_conn, content, mem_type, session_id)?;

            let uncons = db::get_unconsolidated_count(&raw_conn)?;
            if uncons >= config.consolidation.auto_micro_threshold as i64 {
                let _ = sleep::micro_sleep(&raw_conn, &config);
            }

            Ok(format!("Saved memory #{} (type: {})", id, mem_type))
        }
        "cortex_recall" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let memories = db::recall_memories(&raw_conn, query, limit)?;
            if memories.is_empty() {
                Ok("No memories found matching that query.".to_string())
            } else {
                Ok(serde_json::to_string_pretty(&memories)?)
            }
        }
        "cortex_context" => {
            let compact = args.get("compact").and_then(|v| v.as_bool()).unwrap_or(false);
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            context::format_context(&cons_conn, &raw_conn, compact)
        }
        "cortex_sleep" => {
            let micro = args.get("micro").and_then(|v| v.as_bool()).unwrap_or(false);
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let config = config::load_config(cortex_dir)?;

            if micro {
                let removed = sleep::micro_sleep(&raw_conn, &config)?;
                Ok(format!("Micro sleep complete. Removed {} stale memories.", removed))
            } else {
                let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
                let result = sleep::quick_sleep(&raw_conn, &cons_conn, &config, cortex_dir).await?;
                Ok(format!(
                    "Quick sleep complete. {} consolidations, {} promotions, {} decayed, {} skills updated.",
                    result.consolidations.len(), result.promotions.len(), result.decayed.len(), result.skill_updates.len()
                ))
            }
        }
        "cortex_stats" => {
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let stats = db::get_stats(&raw_conn, &cons_conn)?;
            Ok(serde_json::to_string_pretty(&stats)?)
        }
        _ => anyhow::bail!("Unknown tool: {}", name),
    }
}
