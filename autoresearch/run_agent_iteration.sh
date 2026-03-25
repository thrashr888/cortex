#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

AGENT_BIN="${AGENT_BIN:-hermes}"
PROMPT_FILE="${PROMPT_FILE:-autoresearch/agent_prompt.md}"
BASELINE_JSON="${BASELINE_JSON:-autoresearch/baseline.json}"
EVAL_JSON="${EVAL_JSON:-autoresearch/eval.json}"
LOG_FILE="${LOG_FILE:-autoresearch/agent-iteration.log}"
ALLOW_DIRTY="${ALLOW_DIRTY:-0}"

if ! command -v "$AGENT_BIN" >/dev/null 2>&1; then
  echo "missing agent binary: $AGENT_BIN" >&2
  exit 1
fi

if [[ "$ALLOW_DIRTY" != "1" ]] && [[ -n "$(git status --porcelain)" ]]; then
  echo "refusing to run with a dirty worktree; commit or stash first, or set ALLOW_DIRTY=1" >&2
  exit 2
fi

START_COMMIT="$(git rev-parse HEAD)"
mkdir -p autoresearch autoresearch/logs

./autoresearch/run_eval.sh "$BASELINE_JSON" >/tmp/cortex-autoresearch-baseline.txt
PROMPT_CONTENT="$(cat "$PROMPT_FILE")"

{
  echo "[autoresearch] start_commit=$START_COMMIT"
  echo "[autoresearch] branch=$(git branch --show-current)"
  echo "[autoresearch] baseline_compare=$(python3 autoresearch/score_compare.py "$BASELINE_JSON" "$BASELINE_JSON" | tr '\n' ' ')"
} > "$LOG_FILE"

set +e
case "$(basename "$AGENT_BIN")" in
  codex)
    "$AGENT_BIN" exec --full-auto "$PROMPT_CONTENT" >> "$LOG_FILE" 2>&1
    AGENT_EXIT=$?
    ;;
  claude|claude-code)
    "$AGENT_BIN" -p "$PROMPT_CONTENT" >> "$LOG_FILE" 2>&1
    AGENT_EXIT=$?
    ;;
  opencode)
    "$AGENT_BIN" run "$PROMPT_CONTENT" >> "$LOG_FILE" 2>&1
    AGENT_EXIT=$?
    ;;
  *)
    "$AGENT_BIN" chat -q "$PROMPT_CONTENT" >> "$LOG_FILE" 2>&1
    AGENT_EXIT=$?
    ;;
esac
set -e

if [[ $AGENT_EXIT -ne 0 ]]; then
  echo "agent failed, resetting to $START_COMMIT" | tee -a "$LOG_FILE"
  git reset --hard "$START_COMMIT"
  git clean -fd autoresearch >/dev/null 2>&1 || true
  exit $AGENT_EXIT
fi

if [[ ! -f "$EVAL_JSON" ]]; then
  ./autoresearch/run_eval.sh "$EVAL_JSON" >> "$LOG_FILE"
fi

COMPARE_OUTPUT="$(python3 autoresearch/score_compare.py "$BASELINE_JSON" "$EVAL_JSON")"
echo "$COMPARE_OUTPUT" | tee -a "$LOG_FILE"
VERDICT="$(printf '%s\n' "$COMPARE_OUTPUT" | head -n1)"
END_COMMIT="$(git rev-parse HEAD)"

if [[ "$VERDICT" == "regressed" || "$VERDICT" == "flat" ]]; then
  echo "[autoresearch] verdict=$VERDICT resetting to $START_COMMIT" | tee -a "$LOG_FILE"
  git reset --hard "$START_COMMIT"
  git clean -fd autoresearch >/dev/null 2>&1 || true
  exit 0
fi

if [[ "$END_COMMIT" == "$START_COMMIT" ]]; then
  echo "[autoresearch] improved scores but no commit was created" | tee -a "$LOG_FILE"
  exit 3
fi

echo "[autoresearch] kept commit=$END_COMMIT" | tee -a "$LOG_FILE"
