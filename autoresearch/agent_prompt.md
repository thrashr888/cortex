You are running one autonomous autoresearch iteration for cortex.

Repository: /Users/thrashr888/Workspace/cortex
Workdir: /Users/thrashr888/Workspace/cortex
Branch: autoresearch/2026-03-22-memory-quality

Read first:
- autoresearch/program.md
- autoresearch/README.md

Objective:
- Primary regression gate: maximize total_score
- Secondary hill-climb target: maximize hillclimb_score when total_score is unchanged

Required loop for this single iteration:
1. Inspect current scores, recent results rows, and current benchmark gaps.
2. Form one focused hypothesis.
3. Make one small reviewable change.
4. Run:
   - cargo test
   - ./autoresearch/run_eval.sh autoresearch/eval.json
5. Compare against the pre-iteration baseline.
6. If improved by the keep/discard rules in autoresearch/program.md:
   - update autoresearch/README.md if needed
   - append one row to autoresearch/results.tsv
   - ensure autoresearch/progress.png was regenerated
   - git add the relevant files
   - git commit -m "autoresearch: <short description>"
7. If not improved:
   - revert cleanly to the starting commit for this iteration
   - do not leave a partial commit behind
   - leave the repo unchanged except for allowed local autoresearch scratch files if the wrapper expects them

Constraints:
- Keep diffs small.
- Do not weaken existing benchmark expectations.
- Benchmark hardening is allowed when the current metric is saturated, but only with realistic harder cases.
- Prefer fixes in src/db.rs, src/context.rs, src/eval.rs, or src/sleep.rs.
- Do not schedule cron jobs.
- Finish in a clean state: committed if kept, unchanged if discarded.

Finish by printing a short summary including:
- kept or discarded
- total score delta
- hillclimb score delta
- commit hash if kept
