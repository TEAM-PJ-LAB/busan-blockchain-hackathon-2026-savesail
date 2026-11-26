#!/usr/bin/env bash
# PRD 7.4 acceptance 1: crash at 5 points, resume each time, final chain state equals an uninterrupted run.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=../target/run-local
rm -f $OUT/svm.bin
npx tsx run/run.ts local --quick >/dev/null
cp $OUT/run.json /tmp/savesail_ref_run.json
rm -f $OUT/svm.bin
crashes=0
for i in 1 2 3 4 5; do
  out=$(STOP_AFTER=25 RESUME=${i/1/} npx tsx run/run.ts local --quick 2>&1 || true)
  if grep -q "simulated crash" <<<"$out"; then crashes=$((crashes+1)); fi
done
RESUME=1 npx tsx run/run.ts local --quick >/dev/null
python3 - <<'PY'
import json
a = json.load(open('/tmp/savesail_ref_run.json')); b = json.load(open('../target/run-local/run.json'))
for k in ("deal", "balances", "schedule"):
    assert a[k] == b[k], k
assert [(p["due"], p["outcome"]) for p in a["periods"]] == [(p["due"], p["outcome"]) for p in b["periods"]]
steps = [t["step"] for t in b["txs"]]
assert len(steps) == len(set(steps)), "duplicate transactions after resume"
ref = [t["step"] for t in a["txs"]]
assert sorted(steps) == sorted(ref), (set(ref) - set(steps), set(steps) - set(ref))
print("resume ok:", len(steps), "transactions")
PY
echo "crashes: $crashes"
test "$crashes" -eq 5
