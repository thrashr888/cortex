---
name: cortex-memory-system-architecture
description: Learned patterns for cortex-memory-system-architecture
---

---
title: Cortex Memory System Architecture
description: Design patterns and optimization strategies for the two-tier SQLite memory system
tags: [cortex, memory, sqlite, fts5, architecture]
---

# Cortex Memory System Architecture

## Overview

Cortex uses a two-tier SQLite database architecture to balance immediate episodic memory with long-term consolidated knowledge:

- **raw.db** (gitignored): Episodic memory, transient session data, high-frequency updates
- **consolidated.db** (committed): Long-term memory, consolidated insights, stable reference data

## Memory Optimization: Relevance-Based Loading

### Problem
Large consolidated memory sets cause context bloat during LLM requests, reducing token efficiency and retrieval quality.

### Solution
Implement relevance-based memory compaction with optional semantic filtering:

```rust
// With query parameter: BM25 + confidence scoring (15 results default)
cortex_context --query "FTS5 search optimization"

// Without query: recency-based top-N loading
cortex_context
```

### Implementation Details

1. **FTS5 Configuration**
   - Tokenizer: `porter unicode61` for case-insensitive stemmed matching
   - Scoring: BM25 algorithm with confidence weighting
   - Prefix queries: Use wildcard prefix (`*term`) for partial matching
   - Indexes: Automatic via FTS5 table with triggers on consolidated updates

2. **Search Pipeline** (`search_consolidated` function in db.rs)
   - Parse query into BM25-compatible format
   - Apply confidence-based result ranking
   - Apply recency secondary sort
   - Return limited result set (default 15, configurable)

3. **Fallback Behavior**
   - No query provided → Load top-N by recency timestamp
   - Query provided → Load top-N by BM25 relevance + confidence
   - Always limits context to prevent bloat

## Implementation Checklist

- [ ] Add `query` parameter to `cortex_context` MCP tool
- [ ] Add `--query` flag to CLI interface
- [ ] Create `search_consolidated` FTS5 function in db.rs
- [ ] Modify `format_context()` to conditionally use relevance vs. recency loading
- [ ] Add FTS5 index to consolidated table schema
- [ ] Create update triggers to keep FTS5 index synchronized
- [ ] Set default result limit to 15 (tunable)

## Performance Considerations

- FTS5 prefix queries are more efficient than suffix queries
- Confidence threshold filtering (≥0.7) reduces false positives
- BM25 k1=1.2, b=0.75 defaults sufficient for most cases
- Monitor consolidated.db file growth; consider archival if > 10MB

## Graceful Degradation

If FTS5 search fails or query is malformed:
1. Log warning
2. Fall back to recency-based loading
3. Return results with degraded quality indicator in metadata

