#!/usr/bin/env bash
# Run the push-demo server with secrets injected from fnox keychain.
# Used by `mise run push-demo:serve` and `mise run push-demo:dev`.
# Reads PORT / STORE / HTTP_NU / PLUGIN from env (mise provides defaults).

set -e
cd "$(dirname "$0")/.."

PORT="${PORT:-8080}"
STORE="${STORE:-./.store/push-demo}"
HTTP_NU="${HTTP_NU:-./target/debug/http-nu}"
PLUGIN="${PLUGIN:-./target/debug/nu_plugin_push}"

mkdir -p "$STORE"

# fnox exec injects VAPID_* + PUSH_ADMIN_TOKEN from keychain. Fresh clones
# must run `mise run push:vapid:generate` first.
exec fnox exec -- "$HTTP_NU" \
  --plugin "$PLUGIN" \
  --store "$STORE" \
  ":$PORT" \
  examples/push-demo/serve.nu
