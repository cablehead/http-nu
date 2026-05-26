#!/usr/bin/env bash
# Idempotently create a catch-all CF Email Routing rule that points all
# inbound for $CF_EMAIL_SENDER_DOMAIN at the cf_email_worker. Used by
# `mise run email:routing:rule:apply`.
#
# Reads from env (mise/fnox injects):
#   CF_EMAIL_SENDER_DOMAIN  -- the apex / subdomain accepting mail
#   CLOUDFLARE_API_TOKEN    -- token with Email Routing edit + Zone read
#
# Output: prints what it did. Exit 0 on either "created" or "already exists";
# exit non-zero on any actual error.
#
# Dashboard alternative: Cloudflare -> Email -> Email Routing -> Routes ->
# Create custom address (or "catch-all"). Pick "Send to a Worker" and select
# cf-email-worker.

set -euo pipefail

domain="${CF_EMAIL_SENDER_DOMAIN:-}"
token="${CLOUDFLARE_API_TOKEN:-}"
worker_name="cf-email-worker"

if [ -z "$domain" ]; then
  echo "CF_EMAIL_SENDER_DOMAIN is not set. Run:" >&2
  echo "  mise run email:worker:domain-set -- your-domain.example.com" >&2
  exit 1
fi
if [ -z "$token" ]; then
  echo "CLOUDFLARE_API_TOKEN is not set. Store it in fnox keychain under" >&2
  echo "  HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN, then re-run via mise." >&2
  exit 1
fi

api() {
  curl -fsSL --retry 2 \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    "$@"
}

# ----------------------------------------------------------------------------
# Resolve zone id from domain. CF API: list zones by name.
# ----------------------------------------------------------------------------
zone_json=$(api "https://api.cloudflare.com/client/v4/zones?name=$domain")
if [ "$(echo "$zone_json" | jq -r '.success')" != "true" ]; then
  echo "Zone lookup failed. CF API response:" >&2
  echo "$zone_json" >&2
  exit 1
fi
zone_id=$(echo "$zone_json" | jq -r '.result[0].id // empty')

if [ -z "$zone_id" ]; then
  echo "No zone found for $domain in this Cloudflare account." >&2
  echo "Make sure the zone exists and the API token has Zone:Read permission." >&2
  exit 1
fi

echo "Zone: $domain -> $zone_id"

# ----------------------------------------------------------------------------
# Check existing rules. If a catch-all -> worker rule already targets
# cf_email_worker, we're done.
# ----------------------------------------------------------------------------
rules_json=$(api "https://api.cloudflare.com/client/v4/zones/$zone_id/email/routing/rules")
existing_id=$(echo "$rules_json" | jq -r --arg w "$worker_name" '
  .result[]?
  | select(
      ([.matchers[]? | select(.type == "all")] | length) > 0
      and ([.actions[]? | select(.type == "worker") | .value[]?] | index($w))
    )
  | .id
' | head -1)

if [ -n "$existing_id" ]; then
  echo "Catch-all rule already targets $worker_name (rule id: $existing_id). Nothing to do."
  exit 0
fi

# ----------------------------------------------------------------------------
# Create the catch-all rule.
# ----------------------------------------------------------------------------
echo "Creating catch-all rule -> $worker_name ..."
create_body=$(jq -nc --arg w "$worker_name" '{
  name: "catch-all -> \($w)",
  enabled: true,
  matchers: [{type: "all"}],
  actions: [{type: "worker", value: [$w]}]
}')

create_resp=$(api -X POST \
  "https://api.cloudflare.com/client/v4/zones/$zone_id/email/routing/rules" \
  -d "$create_body")

if [ "$(echo "$create_resp" | jq -r '.success')" != "true" ]; then
  echo "Rule creation failed. CF API response:" >&2
  echo "$create_resp" >&2
  exit 1
fi

echo "Created. All mail to *@$domain will now route to $worker_name."
echo "Verify in dashboard: Email -> Email Routing -> Routes."
