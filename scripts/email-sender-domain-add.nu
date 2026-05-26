#!/usr/bin/env nu
# Idempotently add the configured sender domain to Cloudflare Email Service
# AND publish the DKIM / SPF / DMARC records to DNS. End-to-end automation
# of what was previously a manual dashboard step. Used by
# `mise run email:sender-domain:add`.
#
# Reads from env (mise/fnox injects):
#   CF_EMAIL_SENDER_DOMAIN  -- the subdomain to register (e.g. mail.example.com)
#   CLOUDFLARE_API_TOKEN    -- token with Email:Edit + Zone DNS:Edit + Zone:Read
#
# Flow:
#   1. find the parent zone (longest matching suffix from your account's zones)
#   2. POST the subdomain to Email Service (skip if already present)
#   3. GET the recommended DNS records (DKIM / SPF / DMARC, plus bounce MX)
#   4. POST each record via DNS API (skip if it exists; tolerate Email
#      Routing's MX lock -- TXT records still go in, which is what's
#      required for outbound DKIM signing)
#
# Exit non-zero only if step 1 or 2 fails. DNS conflicts are reported but
# don't fail the run -- bounce MX is nice-to-have, not required for send.

const CF_API = "https://api.cloudflare.com/client/v4"

def auth_headers []: nothing -> list<string> {
    let token = ($env.CLOUDFLARE_API_TOKEN? | default "")
    if ($token | is-empty) {
        print "CLOUDFLARE_API_TOKEN is not set."
        exit 1
    }
    [Authorization $"Bearer ($token)"]
}

# Strip surrounding quotes from a TXT record content. CF's GET returns
# them quoted; POST wants them unquoted (or accepts both, but consistent
# is safer).
def unquote_txt [content: string]: nothing -> string {
    $content | str trim --char '"'
}

# Find the parent zone that contains $subdomain. We list all zones we can
# see and pick the one whose `name` is the longest suffix of $subdomain.
# (CF allows nested zones; longest-suffix wins.)
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
        print $"No CF zone matches ($subdomain). Available zones:"
        $resp.result | get name | each {|n| print $"  ($n)" } | ignore
        exit 1
    }
    $candidates | first
}

# Ensure the subdomain is registered on Email Service for `zone`. Returns
# the subdomain record (with its id).
def ensure_subdomain_registered [zone: any, subdomain: string, headers: list<string>]: nothing -> any {
    let existing_resp = (http get --headers $headers $"($CF_API)/zones/($zone.id)/email/sending/subdomains")
    if ($existing_resp.success? | default false) {
        let existing = ($existing_resp.result | default [] | where name == $subdomain | first)
        if ($existing | is-not-empty) {
            print $"  already registered \(subdomain id: ($existing.id)\)"
            return $existing
        }
    }
    print "  registering with Email Service..."
    let url = $"($CF_API)/zones/($zone.id)/email/sending/subdomains"
    let resp = (http post --content-type application/json --headers $headers $url { name: $subdomain })
    if not ($resp.success? | default false) {
        print "Subdomain registration failed:"
        print ($resp | to nuon --indent 2)
        exit 1
    }
    print $"  registered \(subdomain id: ($resp.result.id)\)"
    $resp.result
}

# Build the request body for a DNS record create. Returns a record we can
# pass straight to `http post`.
def dns_body [rec: any]: nothing -> record {
    let content = if $rec.type == "TXT" {
        unquote_txt $rec.content
    } else {
        $rec.content
    }
    let comment = "email-native: cf_email_worker sender setup"
    if ($rec.priority? | is-not-empty) {
        {
            type: $rec.type,
            name: $rec.name,
            content: $content,
            ttl: 1,
            priority: $rec.priority,
            comment: $comment,
        }
    } else {
        {
            type: $rec.type,
            name: $rec.name,
            content: $content,
            ttl: 1,
            comment: $comment,
        }
    }
}

# Check whether a DNS record matching name+type+content already exists in
# the zone. Avoids "already exists" errors on re-runs.
def dns_record_exists [zone_id: string, body: record, headers: list<string>]: nothing -> bool {
    let url = $"($CF_API)/zones/($zone_id)/dns_records?type=($body.type)&name=($body.name)"
    let resp = (http get --headers $headers $url)
    if not ($resp.success? | default false) {
        return false
    }
    let needle = if $body.type == "TXT" {
        unquote_txt $body.content
    } else {
        $body.content
    }
    $resp.result | default [] | any {|r|
        let r_content = if $r.type == "TXT" { unquote_txt $r.content } else { $r.content }
        $r_content == $needle
    }
}

def main [] {
    let subdomain = ($env.CF_EMAIL_SENDER_DOMAIN? | default "")
    if ($subdomain | is-empty) {
        print "CF_EMAIL_SENDER_DOMAIN is not set. Run:"
        print "  SENDER_DOMAIN=mail.your-domain.com mise run email:worker:domain-set"
        exit 1
    }
    let headers = auth_headers

    # 1. Find the parent zone.
    print $"\n[1/3] resolving parent zone for ($subdomain)..."
    let zone = (find_zone_for $subdomain $headers)
    print $"  zone: ($zone.name) \(id: ($zone.id)\)"

    # 2. Ensure the subdomain is registered on Email Service.
    print $"\n[2/3] registering ($subdomain) on CF Email Service..."
    let sub = (ensure_subdomain_registered $zone $subdomain $headers)

    # 3. Publish DKIM / SPF / DMARC (and bounce MX) records to DNS.
    print $"\n[3/3] publishing DNS records for ($subdomain)..."
    let dns_url = $"($CF_API)/zones/($zone.id)/email/sending/subdomains/($sub.id)/dns"
    let dns_resp = (http get --headers $headers $dns_url)
    if not ($dns_resp.success? | default false) {
        print "  could not fetch DNS records the subdomain wants:"
        print ($dns_resp | to nuon --indent 2)
        exit 1
    }

    mut added = 0
    mut skipped = 0
    mut failed = 0
    for rec in ($dns_resp.result | default []) {
        let body = dns_body $rec
        let label = $"($rec.type) ($rec.name)"
        if (dns_record_exists $zone.id $body $headers) {
            print $"  skip:  ($label) -- already present"
            $skipped = ($skipped + 1)
            continue
        }
        let post_url = $"($CF_API)/zones/($zone.id)/dns_records"
        # --allow-errors lets us get CF's structured error body for 4xx
        # responses (e.g. Email Routing's MX lock) instead of a generic
        # "Network failure" exception.
        let resp = (http post --allow-errors --content-type application/json --headers $headers $post_url $body)
        if ($resp.success? | default false) {
            print $"  ok:    ($label)"
            $added = ($added + 1)
        } else {
            let msg = ($resp.errors? | default [] | get 0?.message? | default "unknown")
            if ($msg | str downcase | str contains "email routing") {
                print $"  defer: ($label) -- ($msg)"
                print "         \(bounce MX; outbound send still works without these\)"
                $failed = ($failed + 1)
            } else {
                print $"  FAIL:  ($label) -- ($msg)"
                $failed = ($failed + 1)
            }
        }
    }

    print ""
    print $"Summary: added=($added) skipped=($skipped) failed=($failed)"
    print ""
    if $failed > 0 {
        print "Some records did not apply. Outbound DKIM signing only needs the TXT"
        print "records (SPF / DKIM / DMARC); MX records are for inbound bounce delivery."
        print "If outbound is all you need, you can ignore failed MX records."
    } else {
        print "Done. Sender domain is fully set up for outbound + bounce handling."
    }
}
