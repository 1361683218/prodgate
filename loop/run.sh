#!/bin/bash
# Ralph Loop driver. Each iteration starts a FRESH agent process with no
# memory of previous rounds — all state lives in progress.md on disk.
set -e
AGENT_CMD="${AGENT_CMD:-claude -p}"
MAX="${MAX_ITERATIONS:-20}"
for ((i=1; i<=MAX; i++)); do
  echo "[loop] iteration $i/$MAX"
  result=$(cat loop/PROMPT.md | $AGENT_CMD 2>&1) || true
  echo "$result"
  if [[ "$result" == *"COMPLETE"* ]]; then
    echo "[loop] COMPLETE after $i iterations"
    exit 0
  fi
  sleep 2
done
echo "[loop] reached iteration limit $MAX without COMPLETE"
exit 1
