#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ITERATIONS="${1:-5}"
AGENT_BIN="${AGENT_BIN:-hermes}"
SLEEP_SECS="${SLEEP_SECS:-2}"
ALLOW_DIRTY="${ALLOW_DIRTY:-0}"
AGENT_TIMEOUT_SECS="${AGENT_TIMEOUT_SECS:-900}"
WORKTREE_BASE="${WORKTREE_BASE:-$ROOT/.git/autoresearch-worktrees}"

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]]; then
  echo "usage: $0 [iterations]" >&2
  exit 2
fi

if [[ "$ALLOW_DIRTY" != "1" ]] && [[ -n "$(git status --porcelain)" ]]; then
  echo "worktree is dirty; commit or stash first before running autonomous loop" >&2
  exit 2
fi

mkdir -p autoresearch/logs "$WORKTREE_BASE"

append_results_row() {
  local src_tsv="$1"
  local dest_tsv="$ROOT/autoresearch/results.tsv"
  if [[ ! -f "$src_tsv" ]]; then
    return 0
  fi
  if [[ ! -f "$dest_tsv" ]]; then
    printf 'commit\ttotal_score\trecall_score\tcontext_score\thillclimb_score\tstatus\tdescription\n' > "$dest_tsv"
  fi
  local last_row
  last_row="$(tail -n 1 "$src_tsv")"
  if [[ -n "$last_row" && "$last_row" != commit$'\t'* ]]; then
    printf '%s\n' "$last_row" >> "$dest_tsv"
  fi
}

for i in $(seq 1 "$ITERATIONS"); do
  echo "=== autoresearch iteration $i/$ITERATIONS ==="
  START_COMMIT="$(git rev-parse HEAD)"
  START_HEAD="$(git rev-parse --short HEAD)"
  TS="$(date +%Y%m%d-%H%M%S)"
  LOG_FILE="$ROOT/autoresearch/logs/iteration-$TS.log"
  WT_DIR="$WORKTREE_BASE/iter-$TS-$i"

  git worktree add --detach "$WT_DIR" "$START_COMMIT" >/dev/null

  set +e
  (
    cd "$WT_DIR"
    AGENT_BIN="$AGENT_BIN" \
    LOG_FILE="$LOG_FILE" \
    ALLOW_DIRTY=1 \
    AGENT_TIMEOUT_SECS="$AGENT_TIMEOUT_SECS" \
    bash autoresearch/run_agent_iteration.sh
  )
  ITER_EXIT=$?
  set -e

  WT_HEAD="$(git -C "$WT_DIR" rev-parse HEAD)"

  if [[ "$ITER_EXIT" -ne 0 && "$ITER_EXIT" -ne 124 ]]; then
    git worktree remove --force "$WT_DIR" >/dev/null 2>&1 || rm -rf "$WT_DIR"
    echo "iteration $i failed with exit $ITER_EXIT; see $LOG_FILE" >&2
    exit 1
  fi

  if [[ "$WT_HEAD" != "$START_COMMIT" ]]; then
    while IFS= read -r commit; do
      [[ -n "$commit" ]] || continue
      git cherry-pick "$commit" >/dev/null
    done < <(git -C "$WT_DIR" rev-list --reverse "$START_COMMIT..$WT_HEAD")
    append_results_row "$WT_DIR/autoresearch/results.tsv"
    ./autoresearch/run_eval.sh autoresearch/eval.json >/dev/null
  fi

  git worktree remove --force "$WT_DIR" >/dev/null 2>&1 || rm -rf "$WT_DIR"

  END_HEAD="$(git rev-parse --short HEAD)"
  if [[ "$ITER_EXIT" -eq 124 ]]; then
    echo "iteration $i timed out and was isolated safely: $START_HEAD -> $END_HEAD"
  else
    echo "iteration $i complete: $START_HEAD -> $END_HEAD"
  fi
  sleep "$SLEEP_SECS"
done

echo "done. latest chart: $ROOT/autoresearch/progress.png"
