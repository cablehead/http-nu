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

echo "=== sse pipeline tests (test-sse.nu) ==="
STORE="$(mktemp -d -t 2048-test-sse-XXXXXX)"
trap "rm -rf $STORE" EXIT
# 15s wraps a potential hang inside the test (e.g. `let s = .cat
# --follow ...` collecting an infinite stream) so the failure mode
# becomes a non-zero exit instead of a CI lockup.
if ! timeout 15 http-nu eval --store "$STORE" "$SCRIPT_DIR/test-sse.nu"; then
  echo "test-sse.nu failed (hang or assertion error)" >&2
  exit 1
fi
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
