---
name: fts5-memory-relevance-retrieval
description: Learned patterns for fts5-memory-relevance-retrieval
---

# FTS5-Based Memory Relevance Retrieval

## Pattern
Implement query-aware memory loading to prevent context bloat in long-term memory systems.

## Implementation Details

### Default Behavior (No Query)
- Load top N memories by recency
- Simple ordering, lowest latency
- Useful for recent context injection

### Query-Based Retrieval
- Use FTS5 full-text search on consolidated memory table
- Implement BM25 algorithm for relevance ranking
- Apply confidence score weighting for result quality
- Limit default results to 15 items

### Database Setup
- Create FTS5 index on consolidated table with porter unicode61 tokenizer
- Add triggers to maintain search index on INSERT/UPDATE/DELETE
- Implement search_consolidated function for parameterized queries

### Integration Points
- Add query parameter to cortex_context MCP tool
- Expose via CLI flag
- Modify format_context to conditionally load relevant vs recency-ordered memories

## Benefits
- Reduces irrelevant context in LLM prompts
- Improves inference quality through targeted memory recall
- Prevents LLM context window saturation
- Maintains configurable limits for predictable performance

## Related Patterns
- FTS5 search configuration with porter tokenizer and BM25 weighting (memory ID 2)
- SQLite WAL mode for concurrent access (memory ID 6)
