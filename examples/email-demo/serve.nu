# email-demo -- http-nu service that fronts cf_email_worker.
#
# Routes:
#   GET  /                              terse status JSON
#   GET  /health                        readiness probe
#   POST /webhooks/email/inbound        HMAC-verified -> xs `email.received`
#   POST /send                          bearer-auth -> xs `email.send.requested`
#
# Two outbound xs topics drive lifecycle. The send-on-event handler in
# lib/send.nu consumes `email.send.requested` and dispatches via `email send`,
# appending `email.send.{outcome}` per result. See docs/email-native/events.md.
#
# Env (injected via fnox/mise):
#   CF_EMAIL_WEBHOOK_HMAC_KEY    -- shared with cf_email_worker; verifies inbound
#   EMAIL_ADMIN_TOKEN            -- gates POST /send
#
# Run via:
#   mise run email-demo:serve

const SCRIPT_DIR = (path self | path dirname)

print $"email-demo: started -- HMAC=($env.CF_EMAIL_WEBHOOK_HMAC_KEY? | default '<unset>' | if $in == '<unset>' { $in } else { 'set' })  ADMIN=($env.EMAIL_ADMIN_TOKEN? | default '<unset>' | if $in == '<unset>' { $in } else { 'set' })"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def json-response [obj: any, --status (-s): int = 200] {
    $obj | to json | metadata set { merge {'http.response': {
        status: $status,
        headers: { "Content-Type": "application/json" }
    }}}
}

def text-response [text: string, --status (-s): int = 200] {
    $text | metadata set { merge {'http.response': {
        status: $status,
        headers: { "Content-Type": "text/plain; charset=utf-8" }
    }}}
}

def bad-request [msg: string] {
    text-response --status 400 $"bad request: ($msg)"
}

def unauthorized [] {
    text-response --status 401 "unauthorized"
}

# Bearer auth against EMAIL_ADMIN_TOKEN. Demo uses a single shared token;
# production should layer session auth on top.
def admin-authorized [req: record] {
    let token = ($env.EMAIL_ADMIN_TOKEN? | default "")
    if ($token | is-empty) { return false }
    let auth = ($req.headers | get -i authorization | default "")
    $auth == $"Bearer ($token)"
}

# Verify HMAC-SHA256 of the request body matches `X-Signature: sha256=<hex>`.
# Shells out to openssl -- nushell 0.112 doesn't have built-in keyed HMAC.
# Note: string-equality compare here is not constant-time. The window is
# microseconds over the network and the demo is intentionally simple; if
# this lands in production, swap for a constant-time helper.
def verify-webhook-hmac [body: binary, signature: string] {
    let key = ($env.CF_EMAIL_WEBHOOK_HMAC_KEY? | default "")
    if ($key | is-empty) { return false }
    let given = ($signature | str replace "sha256=" "")
    let computed = (
        $body
        | ^openssl dgst -sha256 -hmac $key -hex
        | str trim
        | parse "{_prefix}= {hex}"
        | get 0.hex?
        | default ""
    )
    $computed == $given
}

# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

{|req|
    let body = $in

    match $req {
        {method: "GET", path: "/"} => {
            json-response {
                service: "email-demo",
                routes: [
                    "GET /",
                    "GET /health",
                    "POST /webhooks/email/inbound",
                    "POST /send",
                ],
            }
        }

        {method: "GET", path: "/health"} => {
            text-response "ok"
        }

        # Inbound from cf_email_worker. Verifies HMAC over the raw body,
        # then appends the parsed envelope to xs as `email.received` with
        # content = the raw_mime_b64 string the worker sent.
        {method: "POST", path: "/webhooks/email/inbound"} => {
            let sig = ($req.headers | get -i x-signature | default "")
            if ($sig | is-empty) {
                bad-request "missing X-Signature header"
            } else if not (verify-webhook-hmac $body $sig) {
                unauthorized
            } else {
                let payload = (try { $body | into string | from json } catch { null })
                if $payload == null or ($payload.raw_mime_b64? | is-empty) {
                    bad-request "invalid inbound payload"
                } else {
                    let meta = {
                        envelope_from: ($payload.envelope_from? | default ""),
                        envelope_to: ($payload.envelope_to? | default ""),
                        from: ($payload.from? | default null),
                        to: ($payload.to? | default null),
                        subject: ($payload.subject? | default null),
                        message_id: ($payload.message_id? | default null),
                        in_reply_to: ($payload.in_reply_to? | default null),
                        received_at_ms: ($payload.received_at_ms? | default 0),
                    }
                    $payload.raw_mime_b64 | .append "email.received" --meta $meta
                    json-response { ok: true }
                }
            }
        }

        # Admin: enqueue an outbound send. The lib/send.nu handler picks it
        # up from xs and dispatches via the plugin. Auth: Bearer EMAIL_ADMIN_TOKEN.
        {method: "POST", path: "/send"} => {
            if not (admin-authorized $req) {
                unauthorized
            } else {
                let body_str = ($body | into string)
                let parsed = (try { { ok: true, value: ($body_str | from json) } } catch {|e| { ok: false, error: $e.msg } })
                if not $parsed.ok {
                    bad-request $"json: ($parsed.error)"
                } else {
                    let req_record = $parsed.value
                    let missing = ([to from subject text] | where {|f| ($req_record | get -i $f | default "" | is-empty) })
                    if not ($missing | is-empty) {
                        bad-request $"missing required field: ($missing | str join ', ')"
                    } else {
                        let meta = {
                            request_ref: ($req_record.request_ref? | default null),
                            to: $req_record.to,
                            from: $req_record.from,
                        }
                        $body_str | .append "email.send.requested" --meta $meta
                        json-response { ok: true, status: "queued" }
                    }
                }
            }
        }

        _ => {
            text-response --status 404 "not found"
        }
    }
}
