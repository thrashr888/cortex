# Changelog

## [0.4.0] - 2026-03-05

### Added
- **Knowledge graph architecture**: Memories now automatically extract entities (technologies, languages, tools, concepts) and relationships via LLM on save.
- **Entity-based recall**: `cortex recall` tries graph search with 1-hop neighbor expansion before falling back to FTS5 text search.
- **Graph-aware consolidation**: `sleep` and `dream` analyze the entity graph to discover new entities, relationships, and update entity descriptions/confidence.
- **Key Entities section** in context output showing entities with their relationships.
- New database tables: `entities` (with FTS5), `relationships` (with indexes) in `raw.db`.
- `entity_ids` column on `memories` and `consolidated` tables for graph linkage.
- `entity_count` and `relationship_count` in stats output.
- Entity extraction LLM function (`llm::extract_entities`).
- `recall_by_entity()` for graph-based memory retrieval.

### Changed
- `sleep` consolidation prompt now includes current entity graph for smarter pattern discovery.
- `dream` reflection prompt analyzes graph structure for clusters, missing relationships, and contradictions.
- `context` output includes entity names in compact format, full entity details in standard format.
- Stats display now shows entity and relationship counts.
- MCP tool descriptions updated to mention entity extraction and graph search.

### Migration
- Existing databases auto-migrate on first run (adds columns and tables, no data loss).
- No new dependencies — pure SQLite with FTS5.

## [0.3.0] - 2026-02-28

### Added
- **Relevance-based memory loading**: Prevent context bloat with query-aware memory retrieval using FTS5 search.
- `--query` and `--limit` flags on `cortex context` CLI command for targeted memory loading.
- `query` and `limit` parameters on `cortex_context` MCP tool (default limit: 15).
- `search_consolidated()` function with BM25 + confidence scoring for semantic search.
- FTS5 index and triggers on consolidated table for automatic search index maintenance.
- Four new skill documentation files from dream consolidation:
  - `cortex-memory-system-architecture.md`: Two-tier DB architecture and optimization patterns
  - `fts5-memory-relevance-retrieval.md`: Query-aware loading implementation guide
  - `rust-sqlite-patterns.md`: Rusqlite lifetime management and FTS5 patterns
  - `aws-bedrock-integration-guide.md`: Doormat authentication and cross-region inference profiles

### Changed
- `format_context()` now conditionally loads memories by relevance (with query) or recency (without query).
- Default memory limit reduced from 20 to 15 for more efficient context usage.
- Global memories use half the limit (limit/2) to prioritize project-specific context.

## [0.2.1] - 2026-02-26

### Added
- `cortex edit <id> <content>` — update a consolidated memory's content by ID.
- `cortex delete <id>` — remove a consolidated memory by ID.
- Negative IDs target global memories (e.g., `cortex edit -- -1 "new content"`).

## [0.2.0] - 2026-02-26

### Added
- **Global memory layer** (`~/.cortex/`): System-wide memory that persists across all projects. Stores personal preferences, tool choices, coding style, and identity.
- **Automatic global promotion**: During sleep consolidation, the LLM identifies cross-project knowledge (preferences, identity, habits) and promotes it to `~/.cortex/consolidated.db`.
- **Unified recall/context**: `recall` and `context` transparently search both project and global memory, merging results.
- **Global stats**: `cortex stats` shows both project and global counts. `cortex stats --global` for global-only view.
- **Auto global dream**: When global memory has 5+ entries and hasn't been dreamed in 24h, automatically runs a dream pass during project sleep.
- **Dedup on global promotion**: Skips inserting duplicate content into global store.
- **MCP global support**: `cortex_save` gains `global` parameter; `cortex_recall`, `cortex_context`, `cortex_stats` include global data automatically.
- **Lazy init**: `~/.cortex/` created automatically on first global promotion (no manual setup needed).
- `--global` / `-g` flag on `sleep` and `dream` for explicit global store operations.

### Changed
- `context` output includes "Global Knowledge" and "Global Skills" sections.
- `wake` blends global context into session start output.

## [0.1.0] - 2026-02-26

### Added
- Core CLI: `init`, `save`, `recall`, `stats`, `sleep`, `dream`, `wake`, `context`, `mcp`.
- Two-database architecture: `raw.db` (episodic, FTS5) + `consolidated.db` (long-term).
- FTS5 full-text search with porter stemming and recency weighting.
- Sleep consolidation: micro (SQL-only dedup/decay) and quick (1 LLM call).
- Dream: deep reflection with cross-session pattern mining.
- Auto-generated skill files in `.cortex/skills/`.
- MCP stdio server with 5 tools (`cortex_save`, `cortex_recall`, `cortex_context`, `cortex_sleep`, `cortex_stats`).
- AWS Bedrock support with SigV4 signing (alongside direct Anthropic API).
- Cross-platform release builds (linux amd64/arm64, macOS amd64/arm64).
- Claude Code hooks integration for automatic consolidation.
