# cortex autoresearch program

This repository already contains an evaluation harness for memory quality.
Your job is to improve the benchmark score while keeping changes reviewable.

## Scope

You may edit:
- `src/db.rs`
- `src/context.rs`
- `src/sleep.rs`
- `src/eval.rs`
- `eval/benchmark.json` only if the human explicitly asks to expand or fix the benchmark

Do not edit unrelated files unless required to make the benchmark runnable.

## Primary metric

Run:

```bash
cargo run -- eval --json
```

The objective is to maximize `total_score`.
Higher is better.
The benchmark now includes distractor-heavy retrieval, trace-derived preference ranking, and contradiction-resolution cases.

Treat the benchmark as fixed. Do not overfit by deleting difficult cases or weakening expectations without human approval.

## Setup

1. Create a fresh branch like `autoresearch/<date>-memory-quality`.
2. Confirm `cargo run -- eval --json` works.
3. `autoresearch/results.tsv` is already scaffolded. Keep it untracked.
4. Use `autoresearch/run_eval.sh` to run the benchmark and print the key scores.

## Experiment loop

Repeat autonomously:

1. Inspect the current benchmark output.
2. Form one focused hypothesis.
   - retrieval ranking in `src/db.rs`
   - project/global blending behavior
   - context selection in `src/context.rs`
   - consolidation heuristics in `src/sleep.rs`
3. Make a small change.
4. Run:

```bash
cargo test
autoresearch/run_eval.sh autoresearch/eval.json
```

5. Extract scores from `autoresearch/eval.json`.
6. Log a row to `autoresearch/results.tsv`.
7. If `total_score` improved, keep the commit.
8. If `total_score` regressed or stayed flat without a clear simplification win, revert.

## Guardrails

- Prefer small diffs.
- Do not add network-dependent evaluation.
- Do not require LLM credentials for the benchmark.
- Simplicity matters: an equal score with simpler logic can be a keep.
- If you change scoring logic in `src/eval.rs`, explain why in the commit message and ensure the benchmark remains a fair quality signal.

## Output expectations

For each kept experiment, commit the code change.
For discarded experiments, reset the branch back.

The end state should be:
- green tests
- `cargo run -- eval --json` working
- best-known score preserved in git
