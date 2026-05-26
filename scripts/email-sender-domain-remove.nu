#!/usr/bin/env nu
# Mirror of email-sender-domain-add.nu. Removes the sender domain
# registration from Cloudflare Email Service AND deletes the DNS records
# we added (matched by their "email-native: cf_email_worker sender setup"
# comment). Idempotent. Used by `mise run email:sender-domain:remove`.
#
# Reads from env:
#   CF_EMAIL_SENDER_DOMAIN  -- the subdomain to deregister
#   CLOUDFLARE_API_TOKEN    -- token with Email:Edit + Zone DNS:Edit + Zone:Read

const CF_API = "https://api.cloudflare.com/client/v4"
const COMMENT_MARKER = "email-native: cf_email_worker sender setup"

def auth_headers []: nothing -> list<string> {
    let token = ($env.CLOUDFLARE_API_TOKEN? | default "")
    if ($token | is-empty) {
        print "CLOUDFLARE_API_TOKEN is not set."
        exit 1
    }
    [Authorization $"Bearer ($token)"]
}

def find_zone_for [subdomain: string, headers: list<string>]: nothing -> any {
    let resp = (http get --headers $headers $"($CF_API)/zones?per_page=50")
    if not ($resp.success? | default false) {
        print "Zone listing failed:"
        print ($resp | to nuon --indent 2)
        exit 1
    }
    let candidates = (
        $resp.result
        | where {|z|
            let zname = ($z.name? | default "")
            ($subdomain == $zname) or ($subdomain | str ends-with $".($zname)")
        }
        | sort-by {|z| ($z.name | str length) * -1 }
    )
    if ($candidates | is-empty) {
        print $"No CF zone matches ($subdomain)."
        exit 0
    }
    $candidates | first
}

def main [] {
    let subdomain = ($env.CF_EMAIL_SENDER_DOMAIN? | default "")
    if ($subdomain | is-empty) {
        print "CF_EMAIL_SENDER_DOMAIN is not set."
        exit 1
    }
    let headers = auth_headers

    print $"\n[1/2] removing DNS records we added \(matched by comment marker\)..."
    let zone = (find_zone_for $subdomain $headers)
    let dns_url = $"($CF_API)/zones/($zone.id)/dns_records?comment=($COMMENT_MARKER | url encode)&per_page=100"
    let dns_resp = (http get --headers $headers $dns_url)
    let ours = ($dns_resp.result? | default [])
    if ($ours | is-empty) {
        print "  none found"
    } else {
        for rec in $ours {
            let label = $"($rec.type) ($rec.name)"
            let del_url = $"($CF_API)/zones/($zone.id)/dns_records/($rec.id)"
            let del_resp = (
                try { http delete --headers $headers $del_url }
                catch {|e| { success: false, errors: [{message: $e.msg}] } }
            )
            if ($del_resp.success? | default false) {
                print $"  deleted: ($label)"
            } else {
                let msg = ($del_resp.errors? | default [] | get 0?.message? | default "unknown")
                print $"  FAIL:    ($label) -- ($msg)"
            }
        }
    }

    print $"\n[2/2] deregistering ($subdomain) from CF Email Service..."
    let subs_resp = (http get --headers $headers $"($CF_API)/zones/($zone.id)/email/sending/subdomains")
    let target = (
        $subs_resp.result? | default [] | where name == $subdomain | first
    )
    if ($target | is-empty) {
        print "  not registered; nothing to do"
    } else {
        let del_url = $"($CF_API)/zones/($zone.id)/email/sending/subdomains/($target.id)"
        let del = (
            try { http delete --headers $headers $del_url }
            catch {|e| { success: false, errors: [{message: $e.msg}] } }
        )
        if ($del.success? | default false) {
            print $"  deregistered \(was id: ($target.id)\)"
        } else {
            let msg = ($del.errors? | default [] | get 0?.message? | default "unknown")
            print $"  FAIL: ($msg)"
            exit 1
        }
    }

    print ""
    print "Sender domain teardown complete."
}
