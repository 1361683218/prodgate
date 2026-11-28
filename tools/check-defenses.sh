#!/usr/bin/env bash
# check-defenses.sh -- audit that prodgate's *external* defenses are live.
#
# REDTEAM.md concludes: local `all` is an AI self-check, not a security
# boundary. The real boundary is CI on a clean checkout plus git protection.
# This script checks those external defenses are actually enabled — because
# a ruleset can exist but be disabled, which looks identical to "protected"
# if you only glance at the GitHub UI.
#
# Read-only. Requires curl + python3. No admin token needed for the public
# rulesets endpoint; CODEOWNERS/CI are checked against the local checkout.

set -u
REPO="${GH_REPO:-1361683218/prodgate}"
BASE="${GITHUB_API:-https://api.github.com}"
fail=0

note() { printf '%s\n' "$*"; }
ok()   { printf '  OK      %s\n' "$*"; }
bad()  { printf '  FAIL    %s\n' "$*"; fail=$((fail + 1)); }

note "prodgate defense audit — $REPO"
note ""

note "[1] Branch protection ruleset"
rules=$(curl -s "$BASE/repos/$REPO/rulesets" 2>/dev/null)
if [ -z "$rules" ]; then
  bad "could not read rulesets (network or auth)"
else
  printf '%s' "$rules" | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    print("  FAIL    rulesets response is not JSON"); sys.exit(1)
if isinstance(data, dict):
    print("  FAIL    rulesets API error: %s" % data.get("message", "unknown")); sys.exit(1)
active = [r for r in data if r.get("enforcement") == "active" and r.get("target") == "branch"]
if not active:
    print("  FAIL    no active branch ruleset (rulesets exist but none are enforced)")
    for r in data:
        print("          - %s: %s" % (r.get("name"), r.get("enforcement")))
    sys.exit(1)
print("  OK      active ruleset(s): %s" % ", ".join(r["name"] for r in active))
' || fail=$((fail + 1))
fi

note "[2] CODEOWNERS covers judge files"
if [ -f .github/CODEOWNERS ]; then
  missing=""
  for p in "tests/" "src/check.rs" "src/mutation.rs" "src/cases.rs" "src/fixture.rs" ".github/workflows/"; do
    grep -qF "$p" .github/CODEOWNERS || missing="$missing $p"
  done
  if [ -z "$missing" ]; then ok "all judge paths declared"; else bad "missing judge paths:$missing"; fi
else
  bad ".github/CODEOWNERS absent"
fi

note "[3] CI gate present"
if [ -f .github/workflows/ci.yml ]; then
  if grep -q "cargo run --locked --bin prodgate -- all" .github/workflows/ci.yml; then
    ok "CI runs the full gate with explicit --bin"
  else
    bad "CI does not run the full gate (or lacks --bin prodgate)"
  fi
  # Exit codes must be propagated. Either pipefail (for `cmd | tee`) or
  # explicit `code=$?` + `exit $code` (for redirected commands) is sound.
  if grep -q "set -o pipefail" .github/workflows/ci.yml \
     || { grep -q 'code=\$?' .github/workflows/ci.yml && grep -q 'exit \$code' .github/workflows/ci.yml; }; then
    ok "command failures are propagated (pipefail or explicit exit code)"
  else
    bad "CI may hide a failed command (no pipefail and no explicit exit code)"
  fi
else
  bad "CI workflow absent"
fi

note "[4] Judge files stay out of git"
tracked=$(git ls-files loop/judge.sh loop/PROMPT.md loop/run.sh 2>/dev/null || true)
if [ -z "$tracked" ]; then
  ok "judge harness is human-held, not version-controlled"
else
  bad "judge harness is in git: $tracked"
fi

note ""
if [ "$fail" -eq 0 ]; then
  note "ALL EXTERNAL DEFENSES VERIFIED"
  exit 0
else
  note "$fail defense check(s) failed — see REDTEAM.md"
  exit 1
fi
