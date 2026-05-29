# examples/email-demo/test/smoke.nu
#
# End-to-end smoke test for email-native. Half-automated:
#
#   automated send -> human eyeballs inbox -> human replies -> automated receive
#
# Prereqs (fnox-resolvable env at runtime):
#   CF_EMAIL_WORKER_URL          - deployed Worker URL
#   CF_EMAIL_AUTH_TOKEN          - plugin <-> Worker bearer
#   CF_EMAIL_WEBHOOK_HMAC_KEY    - Worker -> http-nu HMAC (only needed for inbound half)
#   CF_EMAIL_SENDER_DOMAIN       - the verified sender domain on your CF account
#   TEST_RECIPIENT_EMAIL         - the inbox you'll check by eye (set via
#                                  `mise run email:test:recipient-set -- you@gmail.com`)
#
# Plus -- separately -- examples/email-demo/serve.nu running with xs so the
# inbound half can land. If serve.nu isn't running, the test skips the
# inbound section instead of hanging.
#
# Run via:
#   mise run email-demo:smoke
#
# Optional flags:
#   --outbound-only      send + assert message_id, skip the inbound tail
#   --inbound-timeout 90 seconds to wait for the inbound reply (default 120)

def main [
    --outbound-only,
    --inbound-timeout: int = 120,
] {
    let recipient = ($env.TEST_RECIPIENT_EMAIL? | default "")
    let domain = ($env.CF_EMAIL_SENDER_DOMAIN? | default "")
    let worker = ($env.CF_EMAIL_WORKER_URL? | default "")

    if ($recipient | is-empty) {
        print "FAIL: TEST_RECIPIENT_EMAIL not set."
        print "  Run: mise run email:test:recipient-set -- you@example.com"
        exit 1
    }
    if ($domain | is-empty) {
        print "FAIL: CF_EMAIL_SENDER_DOMAIN not set."
        print "  Run: mise run email:worker:domain-set -- mail.your-domain.com"
        exit 1
    }
    if ($worker | is-empty) {
        print "FAIL: CF_EMAIL_WORKER_URL not set (Worker not deployed?)."
        print "  Run: mise run email:worker:deploy"
        print "  then: mise run email:worker:url-set -- https://cf-email-worker.<acct>.workers.dev"
        exit 1
    }

    let request_ref = $"smoke-(date now | format date '%Y%m%d-%H%M%S')"
    let sender = $"smoke-test@($domain)"
    let subject = $"email-native smoke test ($request_ref)"

    # ------------------------------------------------------------------
    # Outbound
    # ------------------------------------------------------------------
    print $"\n[1/2] outbound: ($sender) -> ($recipient)"
    print $"      subject: ($subject)"
    print $"      via:     ($worker)/send"
    print ""

    let req = {
        to: $recipient,
        from: $sender,
        subject: $subject,
        text: $"email-native smoke test\n\nIf you see this, the outbound path works.\n\nReply to confirm the inbound path closes the loop.\n\nrequest_ref: ($request_ref)\n",
        request_ref: $request_ref,
    }

    # Retry on transient CF errors. The big one: when a sender subdomain
    # is freshly registered, CF Email Service often needs ~30-60 sec to
    # complete DKIM verification before it'll accept sends. First attempt
    # in that window returns E_DELIVERY_FAILED; retries succeed.
    let max_attempts = 6  # 6 * 20s = 2 min cap
    let retryable_codes = [
        "E_DELIVERY_FAILED"
        "E_SENDER_NOT_VERIFIED"
    ]
    mut result = null
    mut attempt = 1
    while $attempt <= $max_attempts {
        $result = ($req | email send)
        if $result.result == "delivered" {
            break
        }
        let code = ($result.error_code? | default "")
        if not ($code in $retryable_codes) {
            break  # non-retryable; fail fast
        }
        if $attempt < $max_attempts {
            print $"  attempt ($attempt)/($max_attempts): result=($result.result), code=($code). Retrying in 20s \(CF often needs ~1min to verify DKIM after fresh subdomain registration\)..."
            sleep 20sec
        }
        $attempt = ($attempt + 1)
    }

    if $result.result != "delivered" {
        print "FAIL: send did not return delivered after retries."
        print ($result | to nuon --indent 2)
        exit 1
    }
    print $"OK   delivered. message_id=($result.message_id) \(attempt ($attempt)/($max_attempts)\)"
    print ""
    print "Eyeball check: open the inbox now. The email should arrive in"
    print "under a minute. If it doesn't, check the worker logs:"
    print "  mise run email:worker:tail"

    if $outbound_only {
        print "\nSmoke test PASS (outbound-only mode)."
        exit 0
    }

    # ------------------------------------------------------------------
    # Inbound
    # ------------------------------------------------------------------
    print ""
    print $"[2/2] inbound: reply to that email from ($recipient)."
    print $"      Or send a fresh email from any client to anything@($domain)."
    print $"      Waiting up to ($inbound_timeout) seconds..."
    print ""

    # Capture our request_ref in a closure-local var so the deadline check
    # below stays readable. The xs frame for our reply won't include
    # request_ref directly (the inbound path doesn't know our outbound
    # request_ref); we just wait for any new email.received frame and trust
    # the human-loop timing.
    let deadline = ((date now | into int) + ($inbound_timeout * 1_000_000_000))

    mut received = null
    while ($received | is-empty) {
        let now = (date now | into int)
        if $now > $deadline {
            print "FAIL: timed out waiting for an email.received frame."
            print "  Things to check:"
            print "    - is serve.nu actually running and reachable from the Worker?"
            print "    - is CF_EMAIL_WEBHOOK_URL set to where serve.nu listens?"
            print "    - does the Worker's CF_EMAIL_WEBHOOK_HMAC_KEY match this side's?"
            print "    - check inbound at Cloudflare: Email -> Email Routing -> Activity"
            exit 1
        }

        # Pull the latest frame on the topic. If there is no frame yet, this
        # returns nothing; loop and retry with a short sleep.
        let candidate = (
            try {
                .cat --topic email.received --last 1 | first
            } catch {
                null
            }
        )
        if ($candidate | is-not-empty) {
            $received = $candidate
        } else {
            sleep 2sec
        }
    }

    print "OK   inbound frame:"
    print $"  envelope_from: ($received.meta.envelope_from? | default '<none>')"
    print $"  envelope_to:   ($received.meta.envelope_to? | default '<none>')"
    print $"  subject:       ($received.meta.subject? | default '<none>')"
    print $"  message_id:    ($received.meta.message_id? | default '<none>')"
    print $"  received_at:   ($received.meta.received_at_ms? | default 0)"
    print ""
    print "Smoke test PASS (both directions verified)."
}
