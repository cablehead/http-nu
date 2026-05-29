#!/usr/bin/env bash
# Run every ex:cf:<demo> in turn, capture the first HTTP response,
# emit a CSV summary. ~30-45s per demo (worker-build is slow), so this
# is a several-minute run.
#
# Usage: bash scripts/cf-demos-probe.sh
#
# Output: stdout has a markdown table; stderr has per-demo build/probe logs.

set -uo pipefail

DEMOS=(
  blog
  basic
  cargo-docs
  2048
  workspace-browser
  datastar-counter
  datastar-sdk
  datastar-sdk-test
  mermaid-editor
  templates
  quotes
  tao
  stor
)

BASE="http://127.0.0.1:8787"
RESULTS_FILE="$(mktemp)"
echo "demo|status|note" > "$RESULTS_FILE"

probe_demo() {
  local demo="$1"
  local out_dir
  out_dir="$(mktemp -d)"
  local log="$out_dir/wrangler.log"

  echo ">>> $demo" >&2
  mise run "ex:cf:$demo" >"$log" 2>&1 &
  local pid=$!

  # Wait up to 90s for wrangler to be ready.
  local ready=0
  for i in {1..90}; do
    if grep -q "Ready on" "$log" 2>/dev/null; then ready=1; break; fi
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 1
  done

  if [ "$ready" -eq 0 ]; then
    if grep -q "handler failed to parse\|Parse error" "$log"; then
      echo "$demo|parse-fail|$(grep -oE 'x [^"\\]*' "$log" | head -1 | tr -d '|')" >> "$RESULTS_FILE"
    else
      echo "$demo|build-fail|build never reached Ready" >> "$RESULTS_FILE"
    fi
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    return
  fi

  # Probe / on the worker (default DO).
  local probe="$out_dir/probe.txt"
  curl -s -o "$probe" -w "%{http_code}" --max-time 5 "$BASE/" > "$out_dir/status_code" || echo "TIMEOUT" > "$out_dir/status_code"
  local code
  code="$(cat "$out_dir/status_code")"
  local note
  case "$code" in
    200) note="serves" ;;
    501|500) note="$(head -c 120 "$probe" | tr '\n|' ' ')" ;;
    *)    note="unexpected" ;;
  esac
  echo "$demo|$code|$note" >> "$RESULTS_FILE"

  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
}

for demo in "${DEMOS[@]}"; do
  probe_demo "$demo"
done

echo
echo "## CF demo probe results"
echo
column -t -s '|' < "$RESULTS_FILE"
