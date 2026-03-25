#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT_JSON="${1:-autoresearch/eval.json}"
mkdir -p "$(dirname "$OUT_JSON")"

cargo run -- eval --json > "$OUT_JSON"
python3 - "$OUT_JSON" <<'PY'
import json, sys
path = sys.argv[1]
with open(path) as f:
    data = json.load(f)
print(f"total_score\t{data['total_score']}")
print(f"recall_score\t{data['recall_score']}")
print(f"context_score\t{data['context_score']}")
print(f"hillclimb_score\t{data['hillclimb_score']}")
PY
python3 autoresearch/render_progress.py >/tmp/cortex-progress-path.txt
printf 'progress_png\t%s\n' "$(cat /tmp/cortex-progress-path.txt)"
