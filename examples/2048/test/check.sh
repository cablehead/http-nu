#!/bin/sh
# Run all 2048 example checks: pure-logic unit tests, SSE pipeline,
# snapshot-actor integration, end-to-end browser test, benchmark.
# Exits non-zero on any failure.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

# The actor integration test and the browser e2e both need the local
# debug binary (the PATH `http-nu` may predate `eval --services`).
if [ ! -x "$REPO_ROOT/target/debug/http-nu" ]; then
  echo "missing target/debug/http-nu -- run \`cargo build\` first" >&2
  exit 1
fi

echo "=== unit tests (test.nu) ==="
http-nu eval "$SCRIPT_DIR/test.nu"
echo

echo "=== sse pipeline tests (test-sse.nu) ==="
STORE_SSE="$(mktemp -d -t 2048-test-sse-XXXXXX)"
STORE_ACTOR="$(mktemp -d -t 2048-test-actor-XXXXXX)"
trap "rm -rf $STORE_SSE $STORE_ACTOR" EXIT
# 15s wraps a potential hang inside the test (e.g. `let s = .cat
# --follow ...` collecting an infinite stream) so the failure mode
# becomes a non-zero exit instead of a CI lockup.
if ! timeout 15 http-nu eval --store "$STORE_SSE" "$SCRIPT_DIR/test-sse.nu"; then
  echo "test-sse.nu failed (hang or assertion error)" >&2
  exit 1
fi
echo

echo "=== snapshot-actor integration (test-snapshot-actor.nu) ==="
# Needs --services so the actor dispatcher spawns alongside the eval;
# uses the local debug build (PATH http-nu may predate that flag).
if ! timeout 30 "$REPO_ROOT/target/debug/http-nu" eval --services --store "$STORE_ACTOR" "$SCRIPT_DIR/test-snapshot-actor.nu"; then
  echo "test-snapshot-actor.nu failed" >&2
  exit 1
fi
echo

echo "=== browser e2e (test.mjs) ==="
if [ ! -d "$SCRIPT_DIR/node_modules" ]; then
  echo "missing node_modules -- run \`npm install\` in $SCRIPT_DIR first" >&2
  exit 1
fi
node "$SCRIPT_DIR/test.mjs"
echo

echo "=== benchmark (bench.nu) ==="
nu "$SCRIPT_DIR/bench.nu"
