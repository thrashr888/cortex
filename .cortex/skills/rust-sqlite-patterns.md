---
name: rust-sqlite-patterns
description: Learned patterns for rust-sqlite-patterns
---

---
title: Rust SQLite Implementation Patterns
description: Proven patterns for rusqlite, FTS5, and SQLite concurrency in Rust
tags: [rust, sqlite, rusqlite, fts5, concurrency]
---

# Rust SQLite Implementation Patterns

## Rusqlite Lifetime Management

### Problem
The borrow checker rejects early statement drops within conditional blocks, causing lifetime errors when transitioning from query_map results.

### Solution
Collect query_map results into named variables *before* the drop/scope boundary:

```rust
// ❌ Borrow checker error
let result = if condition {
    let stmt = conn.prepare("SELECT ...")?;
    stmt.query_map([], |row| row.get(0))?.next()
} else {
    None
};

// ✅ Correct: collect before scope exit
let rows = {
    let stmt = conn.prepare("SELECT ...")?;
    stmt.query_map([], |row| row.get(0))?  // closure captures &stmt
        .collect::<Result<Vec<_>, _>>()?   // collect immediately
}; // stmt dropped here, rows are owned values

let result = rows.first();
```

### Key Pattern
1. Create statement in inner scope
2. Consume query_map iterator immediately
3. Collect results into owned types (Vec, Option, etc.)
4. Exit scope and drop statement
5. Use collected values outside scope

## FTS5 Configuration for Memory Recall

### Tokenizer Configuration
```sql
CREATE VIRTUAL TABLE memories_fts USING fts5(
    content,
    tokenize = 'porter unicode61',  -- Case-insensitive, stemmed matching
    content_rowid = 'id'
);
```

### Query Pattern
```rust
// Prefix wildcard queries are faster than infix
let query = format!("{}*", search_term); // "fts5*" for prefix matching
let stmt = conn.prepare(
    "SELECT id, content, rank FROM memories_fts 
     WHERE memories_fts MATCH ? 
     ORDER BY rank DESC, timestamp DESC 
     LIMIT ?"
)?;
```

### BM25 Scoring
- Default parameters: k1=1.2, b=0.75
- Suitable for memory retrieval without tuning
- Combine with confidence field: `rank * confidence_weight`

## SQLite Concurrency: WAL Mode

### Problem
Default rollback journal mode causes writer-blocker behavior in concurrent agent scenarios.

### Solution
Enable WAL (Write-Ahead Logging) for concurrent read access:

```rust
conn.execute("PRAGMA journal_mode = WAL", [])?;
conn.execute("PRAGMA synchronous = NORMAL", [])?;  // Balance safety/performance
conn.execute("PRAGMA wal_autocheckpoint = 1000", [])?; // Tune checkpoint frequency
```

### Behavior
- Multiple readers can access DB while one writer is active
- WAL file `-wal` tracks uncommitted transactions
- Checkpoint merges WAL into main DB periodically
- Ideal for multi-agent architectures with episodic/long-term DB separation

### Monitoring
```bash
# Check WAL status
sqlite3 consolidated.db "PRAGMA wal_checkpoint(RESTART);"

# Monitor -wal file size (should stay < 1MB)
ls -lh consolidated.db*
```

## FTS5 Index Maintenance

### Auto-sync with Triggers
```sql
-- Consolidated table changes sync to FTS5
CREATE TRIGGER consolidated_ai AFTER INSERT ON consolidated BEGIN
  INSERT INTO consolidated_fts(rowid, content) VALUES (NEW.id, NEW.content);
END;

CREATE TRIGGER consolidated_ad AFTER DELETE ON consolidated BEGIN
  DELETE FROM consolidated_fts WHERE rowid = OLD.id;
END;
```

### Rebuild on Corruption
```rust
conn.execute("REBUILD consolidated_fts", [])?;  // Full index rebuild
```

## Error Handling Pattern

```rust
match conn.execute("...", []) {
    Ok(rows) => Ok(rows),
    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
    Err(e) if e.to_string().contains("SQLITE_IOERR") => {
        // Transient I/O error, retry with backoff
        Err(e)
    }
    Err(e) => {
        // Permanent error
        eprintln!("SQL error: {:?}", e);
        Err(e)
    }
}
```

