#!/bin/sh
# Bench `top-players` at increasing scales. Each scale runs in a fresh
# ephemeral xs store so previous-run frames don't pollute the timing.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
HTTP_NU="$REPO_ROOT/target/debug/http-nu"

if [ ! -x "$HTTP_NU" ]; then
  echo "missing target/debug/http-nu -- run \`cargo build\` first" >&2
  exit 1
fi

for scale in 0 1 2; do
  STORE="$(mktemp -d -t 2048-bench-XXXXXX)"
  echo "=== scale $scale (store: $STORE) ==="
  BENCH_SCALE="$scale" "$HTTP_NU" eval --store "$STORE" "$SCRIPT_DIR/bench-leaderboard.nu"
  rm -rf "$STORE"
  echo
done
