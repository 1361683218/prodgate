#!/bin/bash
# Independent judge. The AI must NOT edit this file — that would let it
# redefine "done" to mean whatever it just did (DGM's reward-hacking failure).
# Usage: ./loop/judge.sh [project-root]
set -e
ROOT="${1:-.}"
cd "$ROOT"
cargo build --offline --quiet
cargo test --offline --quiet >/dev/null
cargo clippy --offline --all-targets --quiet -- -D warnings >/dev/null 2>&1
./target/debug/prodgate all . | grep -E "^(replay|mutation|gate|cases|regression|efficacy)"
echo "JUDGE: PASS"
