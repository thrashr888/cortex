# cortex autoresearch

Purpose: keep a compact record of the benchmark-driven improvement loop for cortex so work can resume cleanly across crashes, restarts, or new sessions.

## Goal

Improve memory retrieval/context quality using a deterministic local benchmark:

```bash
cargo run -- eval --json
```

Primary metric: `total_score`

Subscores:
- `recall_score`
- `context_score`

## Current branch

`autoresearch/2026-03-22-memory-quality`

## Kept commits so far

- `0be4661` Add memory-quality eval harness and contradiction filtering
- `799f932` Hide superseded memories from retrieval context
- `1169bdb` Harden eval against type-only preference matches

## Current benchmark status

As of latest run:
- `total_score = 100.0`
- `recall_score = 100.0`
- `context_score = 100.0`

This means the current benchmark is saturated. To continue meaningful autoresearch, first add a harder realistic benchmark case that exposes new headroom.

## What we learned so far

1. Contradictory old consolidated memories should not remain visible once superseded.
   - Added schema support in `consolidated`:
     - `active`
     - `superseded_by`
   - Retrieval/context only show active memories.

2. FTS retrieval could admit irrelevant results via type-only matches.
   Example:
   - query contains `preference`
   - irrelevant memory also has type `preference`
   - content itself has no query overlap

   Fix:
   - focused retrieval now returns no results if the best content-match score is `0`

3. Context blending could leak near-match global distractors into otherwise strong project-scoped answers.
   Example:
   - project memory: `Upload retry cap should stay at 5 seconds...`
   - global memory: `API retry cap can extend to 30 seconds...`
   - query: `upload retry cap`

   The old behavior rendered both, even though the project hit was the clear intended result.

   Fix:
   - when project match strength fully covers the query and beats global strength, clear global context for that query

## Important files

- `autoresearch/program.md`
  - human/program instructions for the loop
- `autoresearch/run_eval.sh`
  - helper to run eval and print key scores
- `autoresearch/results.tsv`
  - local experiment log
- `eval/benchmark.json`
  - benchmark fixture
- `src/eval.rs`
  - benchmark runner and regression tests
- `src/db.rs`
  - retrieval logic
- `src/context.rs`
  - context formatting/routing
- `src/sleep.rs`
  - consolidation logic

## Resume checklist

When resuming autoresearch in a new session:

1. Check branch and recent commits
   ```bash
   git branch --show-current
   git log --oneline -5
   ```

2. Run tests
   ```bash
   cargo test
   ```

3. Run eval
   ```bash
   ./autoresearch/run_eval.sh autoresearch/eval.json
   ```

4. If score is saturated at 100, do not make blind product changes.
   Instead:
   - inspect current benchmark gaps
   - add one realistic harder case
   - add a regression test
   - rerun eval
   - only then optimize product logic

## Logging convention

`autoresearch/results.tsv` is intentionally local/ignored right now (`autoresearch/results.*` in `.gitignore`).

Use it for local notes like:
- synthetic experiment id
- score snapshot
- keep/discard
- one-line description

If we later want a committed history, we can either:
- stop ignoring `results.tsv`, or
- keep the durable summary in this README and commit that

## Good next benchmark gaps

Best next candidates:
- semantically similar project/global memories where only one should appear in context
- project-specific decision outranking a generic global preference with overlapping terms
- redundant multi-hit context packing where the top result is right but the rest of context is noisy
- consolidation behavior that should mark older facts superseded automatically rather than relying on manual state

## Rule for future sessions

If benchmark = 100 and no new harder case exists, the next useful step is benchmark hardening, not speculative retrieval changes.
