# cortex autoresearch program

This repository already contains an evaluation harness for memory quality.
Your job is to improve the benchmark score while keeping changes reviewable.

## Scope

You may edit:
- `src/db.rs`
- `src/context.rs`
- `src/sleep.rs`
- `src/eval.rs`
- `eval/benchmark.json` for realistic benchmark hardening when the current gate or hill-climb metric saturates
- `autoresearch/README.md`
- `autoresearch/results.tsv`
- `autoresearch/run_eval.sh`
- `autoresearch/render_progress.py`

Do not edit unrelated files unless required to make the benchmark runnable.

## Metrics

Run:

```bash
cargo run -- eval --json
```

Optimize with a two-level objective:
1. primary regression gate: maximize `total_score`
2. secondary hill-climb target: maximize `hillclimb_score` when `total_score` is unchanged

Higher is better for both.
The benchmark now includes distractor-heavy retrieval, trace-derived preference ranking, contradiction-resolution cases, and harder ranking/routing edge cases.

Benchmark hardening is explicitly allowed in this repo when either metric saturates. Add harder realistic cases rather than weakening existing expectations.

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

5. Extract scores from `autoresearch/eval.json` and inspect low-scoring or newly added hard cases.
6. Log a row to `autoresearch/results.tsv` and ensure `autoresearch/progress.png` was regenerated.
7. Keep a change if:
   - `total_score` improved, or
   - `total_score` stayed the same and `hillclimb_score` improved, or
   - scores stayed the same but the logic is clearly simpler and you explain why.
8. Revert a change if:
   - `total_score` regressed, or
   - `total_score` stayed flat and `hillclimb_score` regressed, or
   - both stayed flat with no compelling simplification win.

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
