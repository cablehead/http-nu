#!/usr/bin/env nu
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

const CF_API = "https://api.cloudflare.com/client/v4"
const WORKER_NAME = "cf-email-worker"

def main [] {
    let domain = ($env.CF_EMAIL_SENDER_DOMAIN? | default "")
    let token = ($env.CLOUDFLARE_API_TOKEN? | default "")

    if ($domain | is-empty) {
        print "CF_EMAIL_SENDER_DOMAIN is not set. Run:"
        print "  mise run email:worker:domain-set -- your-domain.example.com"
        exit 1
    }
    if ($token | is-empty) {
        print "CLOUDFLARE_API_TOKEN is not set. Store it in fnox keychain under"
        print "  HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN, then re-run via mise."
        exit 1
    }

    let headers = [Authorization $"Bearer ($token)"]

    # ------------------------------------------------------------------------
    # Resolve zone id from domain.
    # ------------------------------------------------------------------------
    let zone_resp = (
        try {
            http get --headers $headers $"($CF_API)/zones?name=($domain)"
        } catch {|e|
            print "Zone lookup HTTP error:"
            print $e.msg
            exit 1
        }
    )
    if not ($zone_resp.success? | default false) {
        print "Zone lookup failed. CF API response:"
        print ($zone_resp | to nuon --indent 2)
        exit 1
    }
    let zone_id = ($zone_resp.result | get 0?.id? | default "")
    if ($zone_id | is-empty) {
        print $"No zone found for ($domain) in this Cloudflare account."
        print "Make sure the zone exists and the API token has Zone:Read permission."
        exit 1
    }
    print $"Zone: ($domain) -> ($zone_id)"

    # ------------------------------------------------------------------------
    # Check existing rules. If a catch-all -> worker rule already targets
    # cf_email_worker, we're done.
    # ------------------------------------------------------------------------
    let rules_resp = (http get --headers $headers $"($CF_API)/zones/($zone_id)/email/routing/rules")
    let existing = (
        $rules_resp.result
        | default []
        | where {|r|
            let is_catch_all = (
                ($r.matchers? | default [] | any {|m| ($m.type? | default "") == "all" })
            )
            let targets_worker = (
                ($r.actions? | default [] | any {|a|
                    ($a.type? | default "") == "worker"
                        and ($WORKER_NAME in ($a.value? | default []))
                })
            )
            $is_catch_all and $targets_worker
        }
        | first
    )

    if ($existing | is-not-empty) {
        print $"Catch-all rule already targets ($WORKER_NAME) (rule id: ($existing.id)). Nothing to do."
        exit 0
    }

    # ------------------------------------------------------------------------
    # Create the catch-all rule.
    # ------------------------------------------------------------------------
    print $"Creating catch-all rule -> ($WORKER_NAME) ..."
    let body = {
        name: $"catch-all -> ($WORKER_NAME)",
        enabled: true,
        matchers: [{type: "all"}],
        actions: [{type: "worker", value: [$WORKER_NAME]}],
    }
    let create_resp = (
        try {
            http post --content-type application/json --headers $headers $"($CF_API)/zones/($zone_id)/email/routing/rules" $body
        } catch {|e|
            print "Rule creation HTTP error:"
            print $e.msg
            exit 1
        }
    )
    if not ($create_resp.success? | default false) {
        print "Rule creation failed. CF API response:"
        print ($create_resp | to nuon --indent 2)
        exit 1
    }

    print $"Created. All mail to *@($domain) will now route to ($WORKER_NAME)."
    print "Verify in dashboard: Email -> Email Routing -> Routes."
}
