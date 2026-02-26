This is cortex — a repo-local cognitive memory system for AI agents, built in Rust.

## Project Memory

This project uses itself for memory. Use the `cortex_save` MCP tool to save learnings after fixing bugs, making decisions, or discovering patterns. Use `cortex_recall` to search past knowledge.

## Architecture

- `src/main.rs` — CLI entry point (clap)
- `src/db.rs` — SQLite + FTS5 operations (raw.db + consolidated.db)
- `src/sleep.rs` — Consolidation (micro: SQL-only, quick: 1 LLM call)
- `src/dream.rs` — Deep reflection (2-3 LLM calls)
- `src/mcp.rs` — MCP stdio server (JSON-RPC)
- `src/llm.rs` — Anthropic API client
- `src/models.rs` — Data structures
- `src/config.rs` — TOML config
- `src/context.rs` — Context formatting
- `src/skills.rs` — Skill file generation
- `src/wake.rs` — Session start catch-up
- `src/init.rs` — Project initialization

## Build & Test

```bash
cargo build
cargo test
cortex init    # if .cortex/ doesn't exist
cortex stats
```
