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
1. Spend at most a few minutes inspecting current scores, recent results rows, and one benchmark gap.
2. Form exactly one focused hypothesis.
3. Make one small reviewable change only.
4. Run exactly once:
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
- Prefer product-logic fixes over benchmark hardening when a plausible code improvement exists.
- Only harden the benchmark if you are genuinely saturated and the new case is realistic and narrowly scoped.
- Prefer fixes in src/db.rs, src/context.rs, src/eval.rs, or src/sleep.rs.
- Do not edit autoresearch runner infrastructure (`autoresearch/agent_prompt.md`, `autoresearch/run_agent_iteration.sh`, `autoresearch/autonomous_loop.sh`, `autoresearch/command_with_timeout.py`) during a normal product-improvement iteration.
- Do not schedule cron jobs.
- Do not use `python -c`, `python3 -c`, or heredoc python in terminal commands; those may trigger safety confirmation. Use existing scripts, read_file/search_files/patch, or normal cargo/bash commands instead.
- Use `python3 autoresearch/score_compare.py autoresearch/baseline.json autoresearch/eval.json` for score comparison if needed.
- Finish in a clean state: committed if kept, unchanged if discarded.

Finish by printing a short summary including:
- kept or discarded
- total score delta
- hillclimb score delta
- commit hash if kept
