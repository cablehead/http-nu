#!/bin/sh
# Run all cedar-admin example checks. Mirrors examples/2048/test/check.sh
# shape: unit tests first, then SSE pipeline tests (when present), then
# browser e2e. Exits non-zero on any failure.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

HTTP_NU="$REPO_ROOT/target/debug/http-nu"
PLUGIN="$REPO_ROOT/target/debug/nu_plugin_cedar"

if [ ! -x "$HTTP_NU" ]; then
  echo "missing $HTTP_NU -- run \`mise run build\` first" >&2
  exit 1
fi
if [ ! -x "$PLUGIN" ]; then
  echo "missing $PLUGIN -- run \`mise run cedar:plugin:build\` first" >&2
  exit 1
fi

echo "=== unit tests (test.nu) ==="
"$HTTP_NU" --plugin "$PLUGIN" eval "$SCRIPT_DIR/test.nu"
echo

# SSE pipeline tests land when projection.nu + serve.nu exist.
if [ -f "$SCRIPT_DIR/test-sse.nu" ]; then
  echo "=== sse pipeline tests (test-sse.nu) ==="
  STORE="$(mktemp -d -t cedar-admin-test-sse-XXXXXX)"
  trap "rm -rf $STORE" EXIT
  if ! timeout 15 "$HTTP_NU" --plugin "$PLUGIN" eval --store "$STORE" "$SCRIPT_DIR/test-sse.nu"; then
    echo "test-sse.nu failed (hang or assertion error)" >&2
    exit 1
  fi
  echo
fi

# Browser e2e lands when serve.nu exists.
if [ -f "$SCRIPT_DIR/test.mjs" ] && [ -f "$REPO_ROOT/examples/cedar-admin/serve.nu" ]; then
  echo "=== browser e2e (test.mjs) ==="
  if [ ! -d "$SCRIPT_DIR/node_modules" ]; then
    echo "missing node_modules -- run \`mise run cedar-admin:test:install\` first" >&2
    exit 1
  fi
  node "$SCRIPT_DIR/test.mjs"
  echo
else
  echo "=== browser e2e skipped (serve.nu not yet present) ==="
fi

echo "all checks passed"
