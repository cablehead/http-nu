#!/usr/bin/env bash
# Start push-demo + a Cloudflare quick tunnel + show the public URL and a QR.
# Used directly: `./scripts/push-demo-dev.sh`. The mise task wrapper has been
# flaky; this script is the canonical way to run the dev flow.
#
# Requires:
#   - http-nu + nu_plugin_push built (`cargo build && cargo build -p nu_plugin_push`)
#   - VAPID populated in fnox (`PUSH_VAPID_SUBJECT=mailto:you@x.com mise run push:vapid:generate`)
#   - cloudflared on PATH (via mise: aqua:cloudflare/cloudflared)
#   - qrencode optional (brew install qrencode)

set -e
cd "$(dirname "$0")/.."

PORT="${PORT:-8080}"
STORE="${STORE:-./.store/push-demo}"

mkdir -p "$STORE"

cleanup() {
  [ -n "$SERVE_PID" ] && kill "$SERVE_PID" 2>/dev/null || true
  [ -n "$TUNNEL_PID" ] && kill "$TUNNEL_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Start the demo server in the background.
./scripts/push-demo-serve.sh > /tmp/push-demo-serve.log 2>&1 &
SERVE_PID=$!

# Wait for it to start responding.
for _ in $(seq 1 50); do
  if curl -sf "http://127.0.0.1:$PORT/health" > /dev/null 2>&1; then break; fi
  sleep 0.2
done

if ! curl -sf "http://127.0.0.1:$PORT/health" > /dev/null 2>&1; then
  echo "push-demo failed to start on :$PORT" >&2
  echo "log:" >&2
  tail -20 /tmp/push-demo-serve.log >&2
  exit 1
fi

echo "push-demo listening on http://127.0.0.1:$PORT"
echo ""
echo "starting cloudflared quick tunnel..."
echo ""

# Pipe cloudflared's stderr through awk to grab the URL the moment it shows up.
cloudflared tunnel --no-autoupdate --url "http://localhost:$PORT" 2>&1 | while IFS= read -r line; do
  echo "$line"
  url=$(printf '%s' "$line" | grep -oE 'https://[a-zA-Z0-9.-]+\.trycloudflare\.com' | head -1)
  if [ -n "$url" ]; then
    echo ""
    echo "============================================"
    echo "  Open this URL on your phone:"
    echo "  $url"
    echo "============================================"
    echo ""
    if command -v qrencode > /dev/null 2>&1; then
      qrencode -t ANSIUTF8 "$url"
    else
      echo "(install qrencode for a QR code: brew install qrencode)"
    fi
    echo ""
  fi
done
