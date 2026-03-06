---
name: rust-cli-full-text-search
description: Learned patterns for rust-cli-full-text-search
---

# Building Full-Text Search in Rust CLI Tools

## Overview
SQLite's FTS5 extension provides powerful full-text search capabilities for Rust CLI applications.

## Key Patterns

### FTS5 Configuration
- Use `porter` with `unicode61` tokenizer for case-insensitive, stemmed matching
- Enable prefix queries with wildcard syntax (`*term`)
- Apply BM25 scoring combined with recency weighting for effective recall
- Implement FTS5 indexes and triggers for automatic index maintenance

### Integration with tokio
- tokio is the standard async runtime for Rust
- Suitable for both CLI tools and web servers
- Enables concurrent operations and graceful degradation patterns

### Memory Management
- Use rusqlite query_map with proper lifetime management
- Collect results into named variables before dropping blocks to avoid borrow checker errors
- Implement relevance-based loading using FTS5 with BM25 scoring for context optimization

### Implementation Strategy
- Query parameter enables semantic filtering (default 15 results)
- Fallback to recency-based top-N when no query provided
- Use search_consolidated function with confidence scoring for ranked retrieval
- Prevents context bloat in memory consolidation systems

## References
- Existing memories: 14, 15 (memory consolidation patterns)
- Existing memories: 2, 3 (FTS5 and rusqlite patterns)
- Related technologies: tokio async runtime

