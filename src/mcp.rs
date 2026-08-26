use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::config;
use crate::context;
use crate::db;
use crate::init;
use crate::llm;
use crate::models;
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

pub async fn run_mcp_server(cortex_dir: PathBuf, session_id: String, global_dir: Option<PathBuf>) -> Result<()> {
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
        let result = handle_request(&req, &cortex_dir, &session_id, &global_dir).await;

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

async fn handle_request(req: &JsonRpcRequest, cortex_dir: &PathBuf, session_id: &str, global_dir: &Option<PathBuf>) -> Result<Value> {
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
                    "description": "Save a learning, decision, or pattern to project memory. Automatically extracts entities and relationships. Use global=true for cross-project knowledge like personal preferences.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "What was learned or observed" },
                            "type": { "type": "string", "description": "Type: bugfix, decision, pattern, preference, observation", "default": "observation" },
                            "global": { "type": "boolean", "description": "Save to global ~/.cortex/ instead of project (for cross-project knowledge)", "default": false },
                            "artifact_refs": { "type": "array", "items": { "type": "string" }, "description": "Stable references to large supporting artifacts. Cortex stores only refs, never artifact payloads." },
                            "session_id": { "type": "string", "description": "Optional stable host session ID; defaults to the MCP server session." }
                        },
                        "required": ["content"]
                    }
                },
                {
                    "name": "cortex_recall",
                    "description": "Search memory for relevant learnings. Searches both project and global memory automatically. Supports entity-based graph search.",
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
                    "description": "Get current memory context for injection into agent prompts. Includes entities, relationships, and both project and global knowledge.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "compact": { "type": "boolean", "description": "Return compact single-line format", "default": false },
                            "query": { "type": "string", "description": "Optional search query to load only relevant memories. If omitted, loads all memories." },
                            "limit": { "type": "integer", "description": "Max number of relevant memories to include (default: 15)", "default": 15 },
                            "budget_tokens": { "type": "integer", "description": "Optional approximate maximum token budget for selected context." },
                            "include_lineage": { "type": "boolean", "description": "Include consolidated memory and evidence IDs for recovery with cortex_expand.", "default": false }
                        }
                    }
                },
                {
                    "name": "cortex_expand",
                    "description": "Recover the exact project observations and opaque artifact refs behind a consolidated memory. Use an ID from cortex_context with include_lineage=true.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "consolidated_id": { "type": "integer", "description": "Project consolidated memory ID" },
                            "source_offset": { "type": "integer", "description": "Zero-based source page offset", "default": 0 },
                            "source_limit": { "type": "integer", "description": "Maximum source observations to return (default 10, max 100)", "default": 10 }
                        },
                        "required": ["consolidated_id"]
                    }
                },
                {
                    "name": "cortex_sleep",
                    "description": "Run memory consolidation. Automatically discovers entities and relationships, and promotes cross-project patterns to global memory.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "micro": { "type": "boolean", "description": "Use micro sleep (SQL-only, no LLM call)", "default": false }
                        }
                    }
                },
                {
                    "name": "cortex_stats",
                    "description": "Get memory health statistics including entity counts, relationship counts, and global memory counts",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        })),
        "tools/call" => {
            let tool_name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            let text = call_tool(tool_name, &args, cortex_dir, session_id, global_dir).await?;
            Ok(serde_json::json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        _ => anyhow::bail!("Unknown method: {}", req.method),
    }
}

async fn call_tool(name: &str, args: &Value, cortex_dir: &PathBuf, session_id: &str, global_dir: &Option<PathBuf>) -> Result<String> {
    match name {
        "cortex_save" => {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let mem_type = args.get("type").and_then(|v| v.as_str()).unwrap_or("observation");
            let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);
            let artifact_refs = string_array_arg(args, "artifact_refs")?;
            let memory_session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(session_id);
            let artifact_suffix = if artifact_refs.is_empty() {
                String::new()
            } else {
                format!(", {} artifact ref(s)", artifact_refs.len())
            };

            if global {
                let gd = init::ensure_global_dir()?;
                let raw_conn = db::open_raw_db(&gd.join("raw.db"))?;
                let id = db::save_memory_with_artifact_refs(&raw_conn, content, mem_type, memory_session_id, &artifact_refs)?;
                Ok(format!("Saved global memory #{} (type: {}{})", id, mem_type, artifact_suffix))
            } else {
                let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
                let config = config::load_config(cortex_dir)?;
                let id = db::save_memory_with_artifact_refs(&raw_conn, content, mem_type, memory_session_id, &artifact_refs)?;

                // Try to extract entities (best-effort)
                let entity_msg = match llm::extract_entities(content, &config).await {
                    Ok(extraction) => {
                        let mut entity_ids = Vec::new();
                        for entity in &extraction.entities {
                            if let Ok(eid) = db::upsert_entity(&raw_conn, &entity.name, &entity.r#type, entity.description.as_deref()) {
                                entity_ids.push(eid);
                            }
                        }
                        if !entity_ids.is_empty() {
                            let _ = db::update_memory_entities(&raw_conn, id, &entity_ids);
                        }
                        for rel in &extraction.relationships {
                            let source = db::get_entity_by_name(&raw_conn, &rel.source).ok().flatten();
                            let target = db::get_entity_by_name(&raw_conn, &rel.target).ok().flatten();
                            if let (Some(s), Some(t)) = (source, target) {
                                let _ = db::upsert_relationship(&raw_conn, s.id, t.id, &rel.r#type, id, rel.confidence);
                            }
                        }
                        if extraction.entities.is_empty() {
                            String::new()
                        } else {
                            format!(", {} entities extracted", extraction.entities.len())
                        }
                    }
                    Err(_) => String::new(),
                };

                let uncons = db::get_unconsolidated_count(&raw_conn)?;
                if uncons >= config.consolidation.auto_micro_threshold as i64 {
                    let _ = sleep::micro_sleep(&raw_conn, &config);
                }

                Ok(format!("Saved memory #{} (type: {}{}{})", id, mem_type, entity_msg, artifact_suffix))
            }
        }
        "cortex_recall" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;

            // Try entity-based recall first, then fall back to FTS
            let mut memories = db::recall_by_entity(&raw_conn, query, true, limit)?;
            if memories.is_empty() {
                memories = db::recall_memories(&raw_conn, query, limit)?;
            }

            // Also search global consolidated DB
            if let Some(gd) = global_dir {
                if let Ok(global_cons) = db::open_consolidated_db(&gd.join("consolidated.db")) {
                    let global_consolidated = db::get_all_consolidated(&global_cons).unwrap_or_default();
                    let query_lower = query.to_lowercase();
                    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                    for m in global_consolidated {
                        let content_lower = m.content.to_lowercase();
                        if query_words.iter().any(|w| content_lower.contains(w)) {
                            memories.push(models::Memory {
                                id: -m.id,
                                content: format!("[global] {}", m.content),
                                r#type: m.r#type,
                                created_at: m.created_at,
                                accessed_at: m.updated_at,
                                access_count: m.access_count,
                                consolidated: true,
                                importance: m.confidence,
                                session_id: None,
                                entity_ids: vec![],
                                artifact_refs: vec![],
                            });
                        }
                    }
                }
            }

            if memories.is_empty() {
                Ok("No memories found matching that query.".to_string())
            } else {
                Ok(serde_json::to_string_pretty(&memories)?)
            }
        }
        "cortex_context" => {
            let compact = args.get("compact").and_then(|v| v.as_bool()).unwrap_or(false);
            let query = args.get("query").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let budget_tokens = args.get("budget_tokens").and_then(|v| v.as_u64()).map(|value| value as usize);
            let include_lineage = args.get("include_lineage").and_then(|v| v.as_bool()).unwrap_or(false);
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let global_cons = global_dir.as_ref().and_then(|gd| {
                db::open_consolidated_db(&gd.join("consolidated.db")).ok()
            });
            context::format_context(
                &cons_conn,
                &raw_conn,
                global_cons.as_ref(),
                compact,
                query,
                limit,
                budget_tokens,
                include_lineage,
            )
        }
        "cortex_expand" => {
            let consolidated_id = args
                .get("consolidated_id")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| anyhow::anyhow!("cortex_expand requires a consolidated_id."))?;
            let source_offset = args.get("source_offset").and_then(|value| value.as_u64()).unwrap_or(0) as usize;
            let source_limit = args
                .get("source_limit")
                .and_then(|value| value.as_u64())
                .unwrap_or(10)
                .clamp(1, 100) as usize;
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let consolidated = db::get_consolidated_by_id(&cons_conn, consolidated_id)?
                .ok_or_else(|| anyhow::anyhow!("No project consolidated memory #{consolidated_id}."))?;
            let sources = db::get_memories_by_ids(&raw_conn, &consolidated.source_ids, source_offset, source_limit)?;
            let next_source_offset = source_offset.saturating_add(source_limit);
            let next_source_offset = (next_source_offset < consolidated.source_ids.len()).then_some(next_source_offset);
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "consolidated": consolidated,
                "source_offset": source_offset,
                "next_source_offset": next_source_offset,
                "sources": sources,
            }))?)
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
                let mut msg = format!(
                    "Quick sleep complete. {} consolidations, {} promotions, {} decayed, {} skills updated.",
                    result.consolidations.len(), result.promotions.len(), result.decayed.len(), result.skill_updates.len()
                );
                if !result.new_entities.is_empty() {
                    msg.push_str(&format!(" {} new entities.", result.new_entities.len()));
                }
                if !result.new_relationships.is_empty() {
                    msg.push_str(&format!(" {} new relationships.", result.new_relationships.len()));
                }
                if !result.global_promotions.is_empty() {
                    msg.push_str(&format!(" {} promoted to global.", result.global_promotions.len()));
                }
                Ok(msg)
            }
        }
        "cortex_stats" => {
            let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db"))?;
            let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db"))?;
            let stats = db::get_stats(&raw_conn, &cons_conn)?;
            let mut stats_json = serde_json::to_value(&stats)?;

            // Add global stats if available
            if let Some(gd) = global_dir {
                if let Ok(global_cons) = db::open_consolidated_db(&gd.join("consolidated.db")) {
                    let gc: i64 = global_cons.query_row("SELECT COUNT(*) FROM consolidated", [], |r| r.get(0)).unwrap_or(0);
                    let gs: i64 = global_cons.query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0)).unwrap_or(0);
                    stats_json["global_consolidated"] = serde_json::json!(gc);
                    stats_json["global_skills"] = serde_json::json!(gs);
                }
            }

            Ok(serde_json::to_string_pretty(&stats_json)?)
        }
        _ => anyhow::bail!("Unknown tool: {}", name),
    }
}

fn string_array_arg(args: &Value, name: &str) -> Result<Vec<String>> {
    let Some(value) = args.get(name) else {
        return Ok(vec![]);
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{name} must be an array of strings."))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("{name} must be an array of strings."))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expand_recovers_lineage_in_bounded_source_pages() {
        let root = std::env::temp_dir().join(format!("cortex-expand-{}", uuid::Uuid::new_v4()));
        let cortex_dir = root.join(".cortex");
        std::fs::create_dir_all(&cortex_dir).unwrap();
        let raw_conn = db::open_raw_db(&cortex_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&cortex_dir.join("consolidated.db")).unwrap();
        let raw_id = db::save_memory_with_artifact_refs(
            &raw_conn,
            "Retain the checkpoint before retrying the upload.",
            "decision",
            "codex-thread-123",
            &["file:///private/tmp/upload-trace.json".to_string()],
        )
        .unwrap();
        let consolidated_id = db::insert_consolidated(
            &cons_conn,
            "Retries preserve the upload checkpoint.",
            "decision",
            &[raw_id],
            1.0,
        )
        .unwrap();

        let response = call_tool(
            "cortex_expand",
            &serde_json::json!({ "consolidated_id": consolidated_id, "source_limit": 1 }),
            &cortex_dir,
            "server-session",
            &None,
        )
        .await
        .unwrap();
        let expanded: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(expanded["consolidated"]["id"], consolidated_id);
        assert_eq!(expanded["sources"][0]["session_id"], "codex-thread-123");
        assert_eq!(
            expanded["sources"][0]["artifact_refs"][0],
            "file:///private/tmp/upload-trace.json"
        );
        assert!(expanded["sources"][0]["content"]
            .as_str()
            .unwrap()
            .contains("checkpoint"));

        drop(raw_conn);
        drop(cons_conn);
        std::fs::remove_dir_all(root).unwrap();
    }
}
