---
name: fts5-search-configuration
description: Learned patterns for fts5-search-configuration
---

# FTS5 Search Configuration

## Pattern
Configure SQLite FTS5 for effective memory recall with porter unicode61 tokenizer and weighted ranking.

## Configuration
```sql
CREATE VIRTUAL TABLE search_index USING fts5(
  content,
  tokenize = 'porter unicode61'
);
```

## Ranking Strategy
- Use BM25 ranking for relevance
- Apply recency weighting to boost recent memories
- Support prefix queries with star wildcard (*)
- Case-insensitive stemmed matching via porter tokenizer

## Benefits
- Effective repo-scoped memory recall
- Natural language query support
- Balanced relevance and freshness

## References
- Pattern IDs: 3, 9, 11

