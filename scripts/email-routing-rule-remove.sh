#!/usr/bin/env bash
# Mirror of email-routing-rule-apply.sh. Find the catch-all rule that points
# at cf_email_worker (created by the apply script) and delete it. Idempotent
# -- silently skips if no matching rule exists. Used by
# `mise run email:routing:rule:remove`.
#
# Reads from env (mise/fnox injects):
#   CF_EMAIL_SENDER_DOMAIN  -- the zone we wired up
#   CLOUDFLARE_API_TOKEN    -- token with Email Routing edit + Zone read
#
# Dashboard alternative: Cloudflare -> Email -> Email Routing -> Routes ->
# select the rule -> Delete.

set -euo pipefail

domain="${CF_EMAIL_SENDER_DOMAIN:-}"
token="${CLOUDFLARE_API_TOKEN:-}"
worker_name="cf-email-worker"

if [ -z "$domain" ]; then
  echo "CF_EMAIL_SENDER_DOMAIN is not set." >&2
  exit 1
fi
if [ -z "$token" ]; then
  echo "CLOUDFLARE_API_TOKEN is not set." >&2
  exit 1
fi

api() {
  curl -fsSL --retry 2 \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    "$@"
}

# ----------------------------------------------------------------------------
# Resolve zone id.
# ----------------------------------------------------------------------------
zone_json=$(api "https://api.cloudflare.com/client/v4/zones?name=$domain")
if [ "$(echo "$zone_json" | jq -r '.success')" != "true" ]; then
  echo "Zone lookup failed. CF API response:" >&2
  echo "$zone_json" >&2
  exit 1
fi
zone_id=$(echo "$zone_json" | jq -r '.result[0].id // empty')
if [ -z "$zone_id" ]; then
  echo "No zone for $domain in this account. Nothing to remove."
  exit 0
fi

# ----------------------------------------------------------------------------
# Find catch-all rule targeting our worker. Same matcher as apply.
# ----------------------------------------------------------------------------
rules_json=$(api "https://api.cloudflare.com/client/v4/zones/$zone_id/email/routing/rules")
rule_id=$(echo "$rules_json" | jq -r --arg w "$worker_name" '
  .result[]?
  | select(
      ([.matchers[]? | select(.type == "all")] | length) > 0
      and ([.actions[]? | select(.type == "worker") | .value[]?] | index($w))
    )
  | .id
' | head -1)

if [ -z "$rule_id" ]; then
  echo "No catch-all rule targeting $worker_name found. Nothing to remove."
  exit 0
fi

# ----------------------------------------------------------------------------
# Delete.
# ----------------------------------------------------------------------------
echo "Deleting rule $rule_id (catch-all -> $worker_name) ..."
del_resp=$(api -X DELETE "https://api.cloudflare.com/client/v4/zones/$zone_id/email/routing/rules/$rule_id")
if [ "$(echo "$del_resp" | jq -r '.success')" != "true" ]; then
  echo "Delete failed. CF API response:" >&2
  echo "$del_resp" >&2
  exit 1
fi
echo "Removed. *@$domain no longer routes to $worker_name via this rule."
