# cortex

Repo-local cognitive memory for AI agents. Gives Claude Code, OpenCode, and other AI coding agents persistent, self-improving memory per project — plus a global memory layer that carries your preferences across all projects.

## Install

```bash
# From source
cargo install --path .

# Or from crates.io (once published)
cargo install cortex

# Or download a release binary from GitHub
# https://github.com/thrashr888/cortex/releases
```

## Quick Start

```bash
# Initialize in any project
cd /path/to/your/project
cortex init

# Save learnings during work
cortex save "Always use eager loading for UserList queries" --type pattern
cortex save "Fixed race condition in upload handler" --type bugfix
cortex save "Chose SQLite over Postgres for simplicity" --type decision

# Search memories (includes global knowledge automatically)
cortex recall "performance"
cortex recall "upload" --limit 5

# Check stats
cortex stats
```

## How It Works

Cortex uses a two-database architecture inspired by how human memory works:

- **raw.db** (gitignored) — Fast episodic memory. Every observation saved during work.
- **consolidated.db** (committed) — Long-term memory. Merged patterns, resolved contradictions, high-confidence learnings.
- **skills/** (committed) — Auto-generated markdown skill files from consolidated patterns.

### Global Memory

Cortex automatically maintains a system-wide memory at `~/.cortex/` for knowledge that applies across all projects — your name, preferred languages, tools, coding style, and workflow habits.

**How it works**: You don't need to do anything special. During sleep consolidation, the LLM identifies personal preferences and cross-project patterns and automatically promotes them to `~/.cortex/`. Recall and context transparently search both project and global memory.

```bash
# Save something to any project
cortex save "I prefer Rust and Go for CLI tools" --type preference

# Sleep consolidates — the LLM promotes cross-project knowledge to global
cortex sleep

# From ANY project, global knowledge shows up automatically
cortex recall "prefer"
# → [global] Language preference: Rust and Go for CLI tools

cortex context
# → includes both project patterns and global knowledge
```

Global dream runs automatically (nightly) to find deeper patterns across your global knowledge. You can also trigger it manually:

```bash
cortex sleep --global    # Consolidate global store explicitly
cortex dream --global    # Deep reflection on global knowledge
cortex stats --global    # View global stats only
```

### Three Modes

**Wake** — Session start. Catches up any unconsolidated memories from interrupted sessions.
```bash
cortex wake
```

**Sleep** — Consolidation. Micro (SQL-only, instant) or Quick (1 LLM call, ~10s). Automatically promotes cross-project knowledge to global.
```bash
cortex sleep --micro    # Dedup + decay, no LLM, instant
cortex sleep            # LLM-powered: consolidate, promote globals, generate skills
```

**Dream** — Deep reflection. Cross-session pattern mining, meta-learning.
```bash
cortex dream
```

## CLI Reference

| Command | Description |
|---------|-------------|
| `cortex init` | Initialize `.cortex/` in current directory |
| `cortex save <text> --type <type>` | Save a memory (types: bugfix, decision, pattern, preference, observation) |
| `cortex recall <query>` | FTS5 search across project + global memory |
| `cortex stats [--global]` | Memory health (counts, last sleep) |
| `cortex sleep [--micro] [--global]` | Run consolidation |
| `cortex dream [--global]` | Deep reflection (2-3 LLM calls) |
| `cortex wake` | Session start catch-up + context output |
| `cortex context [--compact]` | Output memory context for prompt injection |
| `cortex mcp` | Start MCP stdio server |

Add `--json` to `recall` and `stats` for JSON output. Use `--dir <path>` to target a different project.

## MCP Server

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "cortex": {
      "command": "cortex",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

Exposes 5 tools: `cortex_save`, `cortex_recall`, `cortex_context`, `cortex_sleep`, `cortex_stats`. All tools automatically include global memory — recall and context blend both stores, and sleep promotes cross-project patterns to global.

## Claude Code Hooks

Add to `.claude/hooks/hooks.json` for automatic consolidation:

```json
{
  "hooks": {
    "Stop": [{ "command": "cortex sleep --quick", "timeout": 15000 }],
    "SubagentStop": [{ "command": "cortex sleep --micro", "timeout": 5000 }]
  }
}
```

## Configuration

`.cortex/config.toml`:

```toml
[consolidation]
auto_micro_threshold = 10    # Auto micro-sleep after N saves
decay_threshold = 0.1        # Remove low-value consolidated memories
model = "claude-haiku-4-5"  # Model for sleep/dream LLM calls
```

Set `ANTHROPIC_API_KEY` for direct API access, or use AWS credentials (`AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`/`AWS_SESSION_TOKEN` env vars or `~/.aws/credentials`) for Bedrock. Without LLM credentials, only micro sleep (SQL-only) works.

## What Gets Committed

| Path | Git | Purpose |
|------|-----|---------|
| `.cortex/consolidated.db` | committed | Long-term learned patterns |
| `.cortex/skills/*.md` | committed | Auto-generated skill files |
| `.cortex/config.toml` | committed | Settings |
| `.cortex/raw.db` | gitignored | Ephemeral session observations |
| `~/.cortex/` | n/a | Global cross-project memory |

## Architecture

```
Session start → cortex wake (catch up if needed, blend global context)
  ↓
Working... cortex save × N (micro sleep at threshold)
  ↓
Session end → cortex sleep --quick (consolidate + auto-promote to global)
  ↓
Nightly → auto global dream (cross-project pattern mining)
```

~1000 LOC Rust. SQLite + FTS5 for storage, Anthropic API for consolidation, JSON-RPC for MCP.

## License

MIT License. See [LICENSE](LICENSE).
