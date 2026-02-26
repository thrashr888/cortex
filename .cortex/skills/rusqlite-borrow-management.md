---
name: rusqlite-borrow-management
description: Learned patterns for rusqlite-borrow-management
---

# Rusqlite Borrow Management

## Pattern
Manage rusqlite Statement borrows carefully to avoid lifetime errors with query_map.

## Problem
query_map returns an iterator that borrows the Statement. If the iterator isn't collected before the Statement goes out of scope, you get lifetime errors.

## Solution
Collect query_map results into a named variable before exiting the block:

```rust
let results: Vec<_> = stmt.query_map([], |row| {
    // mapping logic
}).and_then(|mapped| mapped.collect::<Result<Vec<_>, _>>()).unwrap();
// Statement can be dropped here safely
```

## References
- Pattern IDs: 5, 15

