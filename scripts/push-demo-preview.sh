#!/usr/bin/env bash
# Wrapper for preview/dev: generates ephemeral VAPID keys and runs the
# push-demo server. Real deployments use mise + fnox; this is just for the
# Claude Code preview integration so launch.json can stay env-agnostic.

set -e
cd "$(dirname "$0")/.."

PORT="${PORT:-8090}"

# Ephemeral keys per process. Don't reuse these for real subscriptions --
# they evaporate when this script exits.
eval "$(cargo run --example gen_vapid -p nu_plugin_push 2>/dev/null)"
export VAPID_SUBJECT="${VAPID_SUBJECT:-mailto:preview@example.com}"
export PUSH_ADMIN_TOKEN="${PUSH_ADMIN_TOKEN:-preview-token-$$}"

mkdir -p .store/push-demo
exec target/debug/http-nu \
  --plugin "$PWD/target/debug/nu_plugin_push" \
  --store .store/push-demo \
  ":$PORT" \
  examples/push-demo/serve.nu
