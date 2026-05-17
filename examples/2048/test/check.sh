#!/bin/sh
# Run all 2048 example checks: pure-logic unit tests, end-to-end browser
# test, and the per-call benchmark. Exits non-zero on any failure.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

echo "=== unit tests (test.nu) ==="
http-nu eval "$SCRIPT_DIR/test.nu"
echo

echo "=== browser e2e (test.mjs) ==="
if [ ! -x "$REPO_ROOT/target/debug/http-nu" ]; then
  echo "missing target/debug/http-nu -- run \`cargo build\` first" >&2
  exit 1
fi
if [ ! -d "$SCRIPT_DIR/node_modules" ]; then
  echo "missing node_modules -- run \`npm install\` in $SCRIPT_DIR first" >&2
  exit 1
fi
node "$SCRIPT_DIR/test.mjs"
echo

echo "=== benchmark (bench.nu) ==="
nu "$SCRIPT_DIR/bench.nu"
