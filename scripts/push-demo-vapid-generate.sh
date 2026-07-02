#!/usr/bin/env bash
# Generate a VAPID keypair + admin token and store them in the fnox keychain.
# Called by `mise run push:vapid:generate` (which depends on push:plugin:build).
# Requires PUSH_VAPID_SUBJECT (mailto: or https: URL). Kept as a bash script so
# the mise task body stays a single nu-parseable command.
#
# NOTE: regenerating VAPID keys silently invalidates every existing browser
# subscription (servers still 201, browsers drop the sub client-side) -- treat
# a re-run as a reset.
set -e
cd "$(dirname "$0")/.."

if [ -z "$PUSH_VAPID_SUBJECT" ]; then
  echo "PUSH_VAPID_SUBJECT must be set (e.g. PUSH_VAPID_SUBJECT=mailto:you@example.com)" >&2
  exit 1
fi

# gen_vapid emits `export VAPID_PUBLIC_KEY=... \n export VAPID_PRIVATE_KEY=...`
eval "$(cargo run --quiet --example gen_vapid -p nu_plugin_push 2>/dev/null)"

# `-p keychain` routes the value to the macOS keychain provider instead of
# writing plaintext back to fnox.toml.
fnox set -p keychain VAPID_PUBLIC_KEY "$VAPID_PUBLIC_KEY"
fnox set -p keychain VAPID_PRIVATE_KEY "$VAPID_PRIVATE_KEY"
fnox set -p keychain VAPID_SUBJECT "$PUSH_VAPID_SUBJECT"
fnox set -p keychain PUSH_ADMIN_TOKEN "$(openssl rand -hex 32)"

echo ""
echo "Stored in fnox keychain. Public key (for browser applicationServerKey):"
echo "$VAPID_PUBLIC_KEY"
