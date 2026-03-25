#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ITERATIONS="${1:-5}"
AGENT_BIN="${AGENT_BIN:-hermes}"
SLEEP_SECS="${SLEEP_SECS:-2}"
ALLOW_DIRTY="${ALLOW_DIRTY:-0}"
AGENT_TIMEOUT_SECS="${AGENT_TIMEOUT_SECS:-240}"

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]]; then
  echo "usage: $0 [iterations]" >&2
  exit 2
fi

if [[ "$ALLOW_DIRTY" != "1" ]] && [[ -n "$(git status --porcelain)" ]]; then
  echo "worktree is dirty; commit or stash first before running autonomous loop" >&2
  exit 2
fi

mkdir -p autoresearch/logs

for i in $(seq 1 "$ITERATIONS"); do
  echo "=== autoresearch iteration $i/$ITERATIONS ==="
  START_HEAD="$(git rev-parse --short HEAD)"
  TS="$(date +%Y%m%d-%H%M%S)"
  LOG_FILE="autoresearch/logs/iteration-$TS.log"

  if ! AGENT_BIN="$AGENT_BIN" LOG_FILE="$LOG_FILE" ALLOW_DIRTY="$ALLOW_DIRTY" AGENT_TIMEOUT_SECS="$AGENT_TIMEOUT_SECS" bash autoresearch/run_agent_iteration.sh; then
    echo "iteration $i failed; see $LOG_FILE" >&2
    exit 1
  fi

  END_HEAD="$(git rev-parse --short HEAD)"
  echo "iteration $i complete: $START_HEAD -> $END_HEAD"
  sleep "$SLEEP_SECS"
done

echo "done. latest chart: $ROOT/autoresearch/progress.png"
