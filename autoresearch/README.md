# cortex autoresearch

Purpose: keep a compact record of the benchmark-driven improvement loop for cortex so work can resume cleanly across crashes, restarts, or new sessions.

## Goal

Improve memory retrieval/context quality using a deterministic local benchmark:

```bash
cargo run -- eval --json
```

Primary gate metric: `total_score`

Gate subscores:
- `recall_score`
- `context_score`

Secondary hill-climb metric:
- `hillclimb_score`
  - raw summed score across cases
  - rewards stronger ranking and cleaner context
  - penalizes extra retrieved distractors and extra context lines

## Current branch

`autoresearch/2026-03-22-memory-quality`

## Kept commits so far

- `0be4661` Add memory-quality eval harness and contradiction filtering
- `799f932` Hide superseded memories from retrieval context
- `1169bdb` Harden eval against type-only preference matches
- latest `autoresearch: handle comma-delimited query terms`

## Current benchmark status

As of latest run:
- `total_score = 100.0`
- `recall_score = 100.0`
- `context_score = 100.0`
- `hillclimb_score = 480.0`

The raw hill-climb metric improved from `460.0 -> 480.0` after adding another realistic delimiter family case and teaching query normalization to split comma-delimited forms like `temp,file,writer` into phrase-friendly pieces.

Current state:
- delimiter-family cases now cover hyphenated, snake_case, camelCase, dotted, slash-delimited, backslash-delimited, namespace-delimited, plus-delimited, and comma-delimited queries
- `autoresearch/run_eval.sh` regenerates `autoresearch/progress.png` each run so progress is visible at a glance
- all current cases are again at `hillclimb_score = 20.0`

So we made another real retrieval improvement, but the current hill-climb metric is saturated again. The next useful step is another benchmark hardening pass or a finer-grained hill-climb component.

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

4. Preference-style queries need to distinguish routing words from topical words.
   Example:
   - project memory: `For uploads, prefer a 5 second retry cap with resumable chunked retries.`
   - global memory: `I prefer Rust and Go for CLI tools.`
   - query: `prefer upload retries`

   The old behavior could let the global memory survive purely because both query and memory contained `prefer`, even though the real topic was uploads/retries.

   Fix:
   - normalize preference variants like `preference/preferences/prefer` to the same routing term
   - ignore routing-only preference terms when stronger topical terms are present
   - keep the old preference-query override only when the global top hit is actually a preference and the competing project hit is not

5. A saturated gate benchmark still needs a raw optimization objective.
   Fix:
   - keep `total_score` as the regression gate
   - add `hillclimb_score` as a raw summed metric across cases
   - reward nDCG-style ranking quality and required-context coverage
   - penalize extra retrieved distractors, disallowed hits, and extra context lines

6. Eval retrieval should use the same scope-routing decision as rendered context.
   Example:
   - context already suppresses a weak project or global distractor
   - eval retrieval still reported the distractor hit
   - hill-climb score stayed artificially low even though surfaced behavior was already correct

   Fix:
   - extract a shared `scope_decision_for_query(...)`
   - apply it in both `src/context.rs` and eval retrieval reporting
   - this raised `hillclimb_score` from `217.5` to `220.0`

7. Entity-neighbor expansion needs a second-stage query filter.
   Example:
   - query: `UserList`
   - canonical memory mentions `UserList`
   - a neighbor-linked `ActivityFeed` memory is pulled in only because of graph adjacency

   The old behavior surfaced both memories even when the neighbor memory had zero textual overlap with the query.

   Fix:
   - keep neighbor expansion for recall breadth
   - but run the resulting memory set back through query-focused ranking/filtering
   - this raised `hillclimb_score` from `220.0` to `240.0`

8. Confidence should break ties for equally matched entity recall results.
   Example:
   - query: `UserList eager`
   - canonical memory: `UserList eager loading should stay enabled for query paths...`
   - distractor: `UserList eager test fixtures should stay tiny...`

   Both memories matched the same query terms, so naive tie-breaking could pick whichever row happened to be newer.

   Fix:
   - propagate benchmark confidence into raw-memory importance during eval seeding
   - break same-score raw-memory ties with importance, then access count, then id
   - this hardened same-entity ranking behavior without weakening broader recall

9. Scope routing needs phrase-aware tie-breaking, not just term overlap counts.
   Example:
   - query: `upload retry cap`
   - project memory: `Upload retry cap should stay at 5 seconds...`
   - global memory: `Uploads have retry and cap rules...`

   Both memories share the same topical terms, but only the project memory contains the exact intended phrase.

   Fix:
   - keep routing based on topical terms
   - but break ties with phrase-aware match score instead of raw term count alone
   - use the shared phrase-aware scope decision in both context rendering and eval retrieval reporting
   - this raised `hillclimb_score` from `280.0` to `300.0`

10. Simple morphological normalization closes real retrieval gaps.
   Example:
   - query: `upload retries 503`
   - canonical memory: `Uploads should retry with exponential jitter after transient 503 responses.`
   - distractor: `Upload progress UI retries animation frames after reconnect.`

   Without normalization, the query term `retries` matched the distractor literally while failing to align with canonical `retry`, so the distractor could outrank or leak alongside the right memory.

   Fix:
   - normalize simple plural forms like trailing `s`
   - normalize `...ies -> ...y`
   - keep special normalization for routing terms like `preference/preferences/prefer`
   - this raised `hillclimb_score` from `240.0` to `280.0`

11. Hyphenated query forms should not beat spaced canonical phrases.
   Example:
   - query: `temp-file writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp-file previews ...`

   Without normalization, the literal hyphenated token `temp-file` matched the distractor more directly while the canonical spaced phrase only matched `writer`.

   Fix:
   - split hyphenated query terms into phrase-friendly pieces like `temp`, `file`
   - keep the original normalized phrase semantics so `temp-file writer` aligns with `temp file writer`
   - this raised `hillclimb_score` from `280.0` to `300.0`

12. Snake_case query forms should behave like spaced phrases.
   Example:
   - query: `temp_file_writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp_file previews ...`

   Without normalization, the literal underscore token `temp_file_writer` failed FTS lookup and query scoring, so the canonical spaced phrase could disappear entirely.

   Fix:
   - split underscore-separated query terms into phrase-friendly pieces like `temp`, `file`, `writer`
   - apply the same underscore splitting in FTS query building and normalized phrase scoring
   - this raised `hillclimb_score` from `320.0` to `340.0`

13. camelCase query forms should behave like spaced phrases.
   Example:
   - query: `tempFileWriter`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for tempFile previews ...`

   Without camelCase-aware normalization, the query was treated like a single fused token, which failed FTS lookup and phrase scoring against the canonical spaced memory.

   Fix:
   - insert camelCase boundaries before lowercasing query terms
   - apply the same camelCase expansion in normalized phrase scoring for retrieval and context routing
   - this raised `hillclimb_score` from `340.0` to `360.0`

14. Dotted query forms should behave like spaced phrases.
   Example:
   - query: `temp.file.writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp.file previews ...`

   Without dotted-term splitting, the query collapsed into a fused token for FTS lookup, so the canonical spaced phrase could disappear entirely.

   Fix:
   - preserve `.` during query-term normalization long enough to split it into phrase-friendly pieces
   - apply the same dotted-term handling in retrieval scoring and context scope routing
   - this raised `hillclimb_score` from `360.0` to `380.0`

15. Slash-delimited query forms should behave like spaced phrases.
   Example:
   - query: `temp/file/writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp/file previews ...`

   Without slash-aware term splitting, the query collapsed into a fused token for FTS lookup, so the canonical spaced phrase could disappear entirely.

   Fix:
   - preserve `/` during query-term normalization long enough to split it into phrase-friendly pieces
   - exclude slash-joined compounds from the topical scoring set once their pieces are available
   - apply the same slash-delimited handling in retrieval scoring and context scope routing
   - this raised `hillclimb_score` from `380.0` to `400.0`

16. Windows-style backslash-delimited query forms should behave like spaced phrases.
   Example:
   - query: `temp\file\writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp\file previews ...`

   Without normalization, the `\` separators were dropped as punctuation before compound splitting, so the query collapsed toward a fused token and the canonical spaced phrase lost important structure.

   Fix:
   - preserve `\` during query-term normalization long enough to split it into phrase-friendly pieces
   - exclude backslash-joined compounds from the topical scoring set once their pieces are available
   - apply the same backslash-delimited handling in retrieval scoring and context scope routing
   - this raised `hillclimb_score` from `400.0` to `420.0`

17. URL-style plus-delimited query forms should behave like spaced phrases.
   Example:
   - query: `temp+file+writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp+file previews ...`

   Without normalization, the `+` separators were discarded before compound splitting, so the query lost the phrase structure needed to align with the canonical spaced memory.

   Fix:
   - preserve `+` during query-term normalization long enough to split it into phrase-friendly pieces
   - exclude plus-joined compounds from the topical scoring set once their pieces are available
   - apply the same plus-delimited handling in retrieval scoring and context scope routing
   - this raised `hillclimb_score` from `420.0` to `460.0`

18. Comma-delimited query forms should behave like spaced phrases.
   Example:
   - query: `temp,file,writer`
   - canonical memory: `... locking the temp file writer.`
   - distractor: `Writer used for temp,file previews ...`

   Without normalization, the comma-delimited phrase stayed fused in context scope routing, so copied punctuation-separated queries could lose the canonical spaced memory.

   Fix:
   - preserve `,` during query-term normalization long enough to split it into phrase-friendly pieces
   - apply the same comma-delimited handling in retrieval scoring and context scope routing
   - this raised `hillclimb_score` from `460.0` to `480.0`

## Important files

- `autoresearch/program.md`
  - human/program instructions for the loop
- `autoresearch/run_eval.sh`
  - helper to run eval and print key scores
- `autoresearch/results.tsv`
  - local experiment log
- `autoresearch/progress.png`
  - generated progress chart from `results.tsv`
- `autoresearch/agent_prompt.md`
  - single-iteration prompt for autonomous agent runs
- `autoresearch/run_agent_iteration.sh`
  - wrapper for one agent iteration with baseline/eval comparison and keep/discard enforcement
- `autoresearch/autonomous_loop.sh`
  - local loop driver for multiple autonomous iterations
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

## Unattended loop

New helper scripts:
- `autoresearch/agent_prompt.md`
  - one-iteration prompt for an autonomous agent
- `autoresearch/score_compare.py`
  - compares baseline vs candidate eval JSON
- `autoresearch/run_agent_iteration.sh`
  - runs one autonomous iteration with baseline capture and score comparison
- `autoresearch/autonomous_loop.sh`
  - repeats multiple iterations and writes per-iteration logs under `autoresearch/logs/`

Recommended usage:

```bash
# Smoke-test the loop wiring without spending model calls
DRY_RUN=1 bash autoresearch/autonomous_loop.sh 1

# Run 5 real autonomous iterations with Hermes
bash autoresearch/autonomous_loop.sh 5

# Run with Codex (often better for unattended code-editing loops)
AGENT_BIN=codex bash autoresearch/autonomous_loop.sh 5

# Same, but with an explicit per-iteration timeout
AGENT_BIN=codex AGENT_TIMEOUT_SECS=180 bash autoresearch/autonomous_loop.sh 5

# Other supported binaries if installed
AGENT_BIN=claude bash autoresearch/autonomous_loop.sh 5
AGENT_BIN=opencode bash autoresearch/autonomous_loop.sh 5
```

Suggested long unattended run:

```bash
nohup bash autoresearch/autonomous_loop.sh 20 > autoresearch/nohup.out 2>&1 &
```

Each iteration should:
- capture a baseline eval
- let the agent make one focused change
- rerun tests/eval
- keep only gate improvements or same-gate hillclimb improvements
- regenerate `autoresearch/progress.png`
- write iteration logs under `autoresearch/logs/`

## Autonomous loop

Once the current branch is committed or otherwise clean, you can let the local loop run multiple autonomous iterations:

```bash
bash autoresearch/autonomous_loop.sh 5
```

Notes:
- the loop refuses to start on a dirty worktree unless you explicitly set `ALLOW_DIRTY=1`
- each iteration runs inside a disposable detached git worktree under `.git/autoresearch-worktrees/`
- timed-out or failed iterations are discarded by removing the disposable worktree, so the main branch stays clean
- only kept commits are cherry-picked back onto the main branch
- the agent subprocess is wrapped by `autoresearch/command_with_timeout.py` (`AGENT_TIMEOUT_SECS`, default `900`)
- worktree iterations reuse the main repo `target/` via `CARGO_TARGET_DIR`, so follow-on runs are much faster than a cold first iteration
- kept iterations are expected to create a real git commit inside the disposable worktree
- progress is refreshed via `autoresearch/progress.png` after each kept eval run
- local `autoresearch/results.tsv` is updated from kept iterations only

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

`autoresearch/results.tsv` and `autoresearch/progress.png` still match ignore rules in `autoresearch/.gitignore`, so the first committed snapshot on a branch needs `git add -f`.

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
