#!/usr/bin/env nu
# Mirror of email-routing-rule-apply.nu. Find the catch-all rule that points
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

const CF_API = "https://api.cloudflare.com/client/v4"
const WORKER_NAME = "cf-email-worker"

def main [] {
    let domain = ($env.CF_EMAIL_SENDER_DOMAIN? | default "")
    let token = ($env.CLOUDFLARE_API_TOKEN? | default "")

    if ($domain | is-empty) {
        print "CF_EMAIL_SENDER_DOMAIN is not set."
        exit 1
    }
    if ($token | is-empty) {
        print "CLOUDFLARE_API_TOKEN is not set."
        exit 1
    }

    let headers = [Authorization $"Bearer ($token)"]

    # ------------------------------------------------------------------------
    # Resolve zone id.
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
        print $"No zone for ($domain) in this account. Nothing to remove."
        exit 0
    }

    # ------------------------------------------------------------------------
    # Find catch-all rule targeting our worker.
    # ------------------------------------------------------------------------
    let rules_resp = (http get --headers $headers $"($CF_API)/zones/($zone_id)/email/routing/rules")
    let rule = (
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

    if ($rule | is-empty) {
        print $"No catch-all rule targeting ($WORKER_NAME) found. Nothing to remove."
        exit 0
    }

    # ------------------------------------------------------------------------
    # Delete.
    # ------------------------------------------------------------------------
    print $"Deleting rule ($rule.id) (catch-all -> ($WORKER_NAME)) ..."
    let del_resp = (
        try {
            http delete --headers $headers $"($CF_API)/zones/($zone_id)/email/routing/rules/($rule.id)"
        } catch {|e|
            print "Delete HTTP error:"
            print $e.msg
            exit 1
        }
    )
    if not ($del_resp.success? | default false) {
        print "Delete failed. CF API response:"
        print ($del_resp | to nuon --indent 2)
        exit 1
    }

    print $"Removed. *@($domain) no longer routes to ($WORKER_NAME) via this rule."
}
