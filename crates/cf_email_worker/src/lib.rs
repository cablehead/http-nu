// cf_email_worker -- Cloudflare Worker handling both outbound (POST /send
// from nu_plugin_email) and inbound (#[event(email)] from CF Email Routing).
//
// Per the feedback memory `workers-rs + D1 gotchas`: NO bare `?` in event
// handlers. Use `match` and `console_error!` so failures show in the tail.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use wasm_bindgen::JsValue;
use worker::*;

type HmacSha256 = Hmac<Sha256>;

#[event(start)]
fn start() {
    // Panic hook -- without it, panics show up as "wasm trap" with no info.
    console_error_panic_hook::set_once();
}

// ----------------------------------------------------------------------------
// Wire format (matches nu_plugin_email::send types exactly).
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SendRequest {
    to: String,
    from: String,
    subject: String,
    text: String,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    reply_to: Option<String>,
    /// Caller-supplied correlation id. Logged on the worker side; the plugin
    /// is responsible for stitching it back into result records.
    #[serde(default)]
    request_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendOk {
    ok: bool,
    message_id: String,
}

#[derive(Debug, Serialize)]
struct SendErr {
    ok: bool,
    error_code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after: Option<u64>,
}

// ----------------------------------------------------------------------------
// Dispatcher
// ----------------------------------------------------------------------------

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let path = req.path();
    match (req.method(), path.as_str()) {
        (Method::Post, "/send") => handle_send(req, env).await,
        (Method::Get, "/health") => Response::ok("ok"),
        _ => Response::error("not found", 404),
    }
}

// ----------------------------------------------------------------------------
// POST /send
// ----------------------------------------------------------------------------

async fn handle_send(mut req: Request, env: Env) -> Result<Response> {
    // 1. Bearer auth. Fail closed.
    if let Err(resp) = check_bearer(&req, &env) {
        return Ok(resp);
    }

    // 2. Parse JSON body.
    let body: SendRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => {
            console_error!("send: invalid JSON body: {e}");
            return err_response(
                "E_BAD_REQUEST",
                format!("invalid JSON body: {e}"),
                400,
                None,
            );
        }
    };

    let request_ref = body.request_ref.clone().unwrap_or_default();

    // 3. Resolve the SEND_EMAIL binding.
    let sender = match env.send_email("EMAIL") {
        Ok(s) => s,
        Err(e) => {
            console_error!("send[{request_ref}]: SEND_EMAIL binding missing: {e}");
            return err_response(
                "E_BINDING_MISSING",
                "SEND_EMAIL binding not configured on this worker".into(),
                500,
                None,
            );
        }
    };

    // 4. Assemble the message via the structured builder.
    let mut builder =
        email::SendEmailBuilder::builder(&body.from, &body.to, &body.subject).text(&body.text);
    if let Some(html) = body.html.as_deref() {
        builder = builder.html(html);
    }
    if let Some(reply_to) = body.reply_to.as_deref() {
        builder = builder.reply_to(reply_to);
    }
    let message = builder.build();

    // 5. Send. The wasm binding returns Result<_, js_sys::Error>; convert to
    // `worker::Error` so we can match on its structured email-error variants
    // (workers-rs already maps the upstream JS `code` field for us -- see
    // worker/src/error.rs `convert_js_error_with_depth`).
    match sender.send_with_builder(&message).await {
        Ok(result) => {
            let body = SendOk {
                ok: true,
                message_id: result.message_id(),
            };
            console_log!(
                "send[{request_ref}]: delivered message_id={}",
                body.message_id
            );
            match Response::from_json(&body) {
                Ok(r) => Ok(r),
                Err(e) => {
                    console_error!("send[{request_ref}]: cannot serialize SendOk: {e}");
                    Response::error("server error", 500)
                }
            }
        }
        Err(e) => {
            let we: worker::Error = e.into();
            let (code, status) = classify(&we);
            let msg = we.to_string();
            console_error!("send[{request_ref}]: cf error code={code} msg={msg}");
            err_response(&code, msg, status, None)
        }
    }
}

fn check_bearer(req: &Request, env: &Env) -> std::result::Result<(), Response> {
    let expected = match env.secret("CF_EMAIL_AUTH_TOKEN") {
        Ok(s) => s.to_string(),
        Err(_) => {
            console_error!("CF_EMAIL_AUTH_TOKEN secret not set");
            return Err(Response::error("server misconfigured", 500)
                .expect("plain-text Response always constructible"));
        }
    };
    let provided = match req.headers().get("authorization") {
        Ok(Some(v)) => v,
        _ => {
            return Err(Response::error("unauthorized", 401)
                .expect("plain-text Response always constructible"));
        }
    };
    let expected_header = format!("Bearer {expected}");
    if constant_time_eq(provided.as_bytes(), expected_header.as_bytes()) {
        Ok(())
    } else {
        Err(Response::error("unauthorized", 401).expect("plain-text Response always constructible"))
    }
}

// ----------------------------------------------------------------------------
// Inbound: forwarded by CF Email Routing -> POST to http-nu webhook.
//
// The Worker doesn't parse the MIME body itself -- it grabs the obvious
// headers, base64-encodes the raw bytes, signs the JSON envelope, and lets
// http-nu (or its consumers) do the heavy lifting downstream. Keeps the
// WASM footprint small and gives downstream consumers full fidelity.
// ----------------------------------------------------------------------------

/// JSON wire format the Worker POSTs to `$CF_EMAIL_WEBHOOK_URL`. http-nu
/// must use the same field names and verify `X-Signature` over the exact
/// bytes of the request body.
#[derive(Debug, Serialize)]
struct InboundEmail {
    /// Envelope From (SMTP MAIL FROM). RFC 5321; may differ from header From.
    envelope_from: String,
    /// Envelope To (SMTP RCPT TO). RFC 5321; the address that resolved to us.
    envelope_to: String,
    /// Header `From:` (display-name + address per RFC 5322).
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    /// Header `To:` -- raw header value.
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    /// Header `Subject:`.
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    /// Header `Message-ID:`. Stable dedupe key.
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    /// Header `In-Reply-To:` (threading).
    #[serde(skip_serializing_if = "Option::is_none")]
    in_reply_to: Option<String>,
    /// Raw RFC 5322 MIME bytes, base64-encoded so the JSON stays UTF-8 even
    /// for messages with binary parts.
    raw_mime_b64: String,
    /// Worker receive time, epoch milliseconds.
    received_at_ms: f64,
}

#[event(email)]
async fn email(message: ForwardableEmailMessage, env: Env, _ctx: Context) -> Result<()> {
    // 1. Required config. Bail (silently logging) if either is missing --
    // returning Err would surface "Email cannot be processed" SMTP-side,
    // which we'd rather avoid for a config error on our side.
    let webhook_url = match env.var("CF_EMAIL_WEBHOOK_URL") {
        Ok(v) => v.to_string(),
        Err(e) => {
            console_error!("inbound: CF_EMAIL_WEBHOOK_URL var missing: {e}");
            return Ok(());
        }
    };
    let hmac_key = match env.secret("CF_EMAIL_WEBHOOK_HMAC_KEY") {
        Ok(s) => s.to_string(),
        Err(e) => {
            console_error!("inbound: CF_EMAIL_WEBHOOK_HMAC_KEY secret missing: {e}");
            return Ok(());
        }
    };

    // 2. Read raw MIME bytes.
    let raw = match message.raw_bytes().await {
        Ok(b) => b,
        Err(e) => {
            console_error!("inbound: cannot read raw bytes: {e}");
            return Ok(());
        }
    };

    // 3. Extract headers we care about. Header lookups are best-effort -- a
    // missing header gives `None`, never an error worth surfacing.
    let headers = message.headers();
    let get_h = |name: &str| -> Option<String> { headers.get(name).ok().flatten() };

    let payload = InboundEmail {
        envelope_from: message.from(),
        envelope_to: message.to(),
        from: get_h("from"),
        to: get_h("to"),
        subject: get_h("subject"),
        message_id: get_h("message-id"),
        in_reply_to: get_h("in-reply-to"),
        raw_mime_b64: base64::engine::general_purpose::STANDARD.encode(&raw),
        received_at_ms: Date::now().as_millis() as f64,
    };

    // 4. Serialize then sign the exact bytes we'll send.
    let body_bytes = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            console_error!("inbound: serialize payload failed: {e}");
            return Ok(());
        }
    };
    let signature = hmac_sha256_hex(hmac_key.as_bytes(), &body_bytes);

    // 5. Build + send the POST. Errors here are best-effort logged; the
    // SMTP transaction is already complete so we can't ask the sender to
    // retry. If retry/durability matters we'd front this with a Queue or
    // Durable Object -- see TASKS.md out-of-scope notes.
    // `Headers::set` mutates through interior mutability (JS-side), so this
    // binding doesn't need `mut`.
    let req_headers = Headers::new();
    if let Err(e) = req_headers.set("content-type", "application/json") {
        console_error!("inbound: set content-type failed: {e}");
        return Ok(());
    }
    if let Err(e) = req_headers.set("x-signature", &format!("sha256={signature}")) {
        console_error!("inbound: set x-signature failed: {e}");
        return Ok(());
    }

    let body_str = match String::from_utf8(body_bytes) {
        Ok(s) => s,
        Err(e) => {
            // serde_json::to_vec always produces valid UTF-8; this is a
            // belt-and-braces fall-through.
            console_error!("inbound: json bytes not UTF-8 (unreachable): {e}");
            return Ok(());
        }
    };

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(req_headers)
        .with_body(Some(JsValue::from_str(&body_str)));

    let request = match Request::new_with_init(&webhook_url, &init) {
        Ok(r) => r,
        Err(e) => {
            console_error!("inbound: build request failed: {e}");
            return Ok(());
        }
    };

    match Fetch::Request(request).send().await {
        Ok(resp) => {
            let status = resp.status_code();
            if (200..300).contains(&status) {
                console_log!("inbound: webhook POST ok status={status}");
            } else {
                console_error!("inbound: webhook POST non-2xx status={status}");
            }
        }
        Err(e) => {
            console_error!("inbound: webhook POST failed: {e}");
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Map a `worker::Error` to (canonical error_code, http_status). The
/// canonical codes here are what the plugin's `Outcome::from_error_code`
/// branches on. Unknown JS errors that carry a `code` field are passed
/// through verbatim so xs consumers can still distinguish them; truly
/// opaque errors fold to `E_UNKNOWN`.
fn classify(err: &worker::Error) -> (String, u16) {
    use worker::Error::*;
    match err {
        RateLimitExceeded(_) => ("E_RATE_LIMIT_EXCEEDED".into(), 429),
        DailyLimitExceeded(_) => ("E_DAILY_LIMIT_EXCEEDED".into(), 429),
        EmailRecipientNotAllowed(_) => ("E_RECIPIENT_NOT_ALLOWED".into(), 400),
        EmailRecipientSuppressed(_) => ("E_RECIPIENT_SUPPRESSED".into(), 400),
        InternalError(_) => ("E_INTERNAL_SERVER_ERROR".into(), 502),
        UnknownJsError { code: Some(c), .. } => {
            // `E_SENDER_NOT_VERIFIED` is the documented CF code for an
            // unverified sender domain; not yet a typed worker::Error
            // variant, so it falls through this branch.
            let status = if c == "E_SENDER_NOT_VERIFIED" {
                400
            } else {
                502
            };
            (c.clone(), status)
        }
        _ => ("E_UNKNOWN".into(), 502),
    }
}

fn err_response(
    code: &str,
    message: String,
    status: u16,
    retry_after: Option<u64>,
) -> Result<Response> {
    let body = SendErr {
        ok: false,
        error_code: code.into(),
        message,
        retry_after,
    };
    let resp = match Response::from_json(&body) {
        Ok(r) => r,
        Err(e) => {
            console_error!("err_response: cannot serialize SendErr: {e}");
            return Response::error("server error", 500);
        }
    };
    Ok(resp.with_status(status))
}

/// Variable-time-safe byte comparison. Worker code runs single-tenant per
/// request, so timing leaks are bounded, but keeping this constant-time
/// removes the only attack surface that wouldn't be obvious from reading.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    // `new_from_slice` only errors for zero-length keys in current hmac. We
    // already require `CF_EMAIL_WEBHOOK_HMAC_KEY` to be set as a secret;
    // an empty value indicates misconfiguration and we don't try to recover.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key must not be empty");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // hmac_sha256_hex -- RFC 4231 test vectors. Pinning these means a
    // crate-bump in `hmac` or `sha2` that changes output would fail loudly
    // rather than silently corrupting our webhook signatures.
    // ------------------------------------------------------------------------

    #[test]
    fn hmac_sha256_rfc4231_test_case_1() {
        // Test case 1 from RFC 4231 section 4.2:
        // Key  = 0x0b * 20, Data = "Hi There"
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        assert_eq!(
            hmac_sha256_hex(&key, data),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_test_case_2() {
        // Test case 2 from RFC 4231 section 4.3: ASCII key, ASCII data.
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        assert_eq!(
            hmac_sha256_hex(key, data),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_sha256_is_deterministic_on_repeated_input() {
        let key = b"webhook-secret-key";
        let msg = br#"{"envelope_from":"a@b","envelope_to":"c@d"}"#;
        let h1 = hmac_sha256_hex(key, msg);
        let h2 = hmac_sha256_hex(key, msg);
        assert_eq!(h1, h2);
        // sha256 output: 64 hex chars
        assert_eq!(h1.len(), 64);
    }

    // ------------------------------------------------------------------------
    // constant_time_eq -- spot-check the basic contract. The constant-time
    // property itself isn't testable here; we just confirm correctness.
    // ------------------------------------------------------------------------

    #[test]
    fn constant_time_eq_equal_inputs() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(
            b"Bearer secret-1234",
            b"Bearer secret-1234"
        ));
    }

    #[test]
    fn constant_time_eq_different_inputs() {
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(!constant_time_eq(b"Bearer x", b"bearer x")); // case sensitive
    }

    // ------------------------------------------------------------------------
    // classify -- the String-payload variants are constructable on the host
    // (no JsValue). UnknownJsError can't be tested here because it carries a
    // raw JsValue; integration tests covering that path live downstream.
    // ------------------------------------------------------------------------

    #[test]
    fn classify_maps_typed_email_errors_to_canonical_codes() {
        let (code, status) = classify(&worker::Error::RateLimitExceeded("oops".into()));
        assert_eq!(code, "E_RATE_LIMIT_EXCEEDED");
        assert_eq!(status, 429);

        let (code, status) = classify(&worker::Error::DailyLimitExceeded("daily".into()));
        assert_eq!(code, "E_DAILY_LIMIT_EXCEEDED");
        assert_eq!(status, 429);

        let (code, status) = classify(&worker::Error::EmailRecipientNotAllowed("nope".into()));
        assert_eq!(code, "E_RECIPIENT_NOT_ALLOWED");
        assert_eq!(status, 400);

        let (code, status) = classify(&worker::Error::EmailRecipientSuppressed("on list".into()));
        assert_eq!(code, "E_RECIPIENT_SUPPRESSED");
        assert_eq!(status, 400);

        let (code, status) = classify(&worker::Error::InternalError("transient".into()));
        assert_eq!(code, "E_INTERNAL_SERVER_ERROR");
        assert_eq!(status, 502);
    }

    #[test]
    fn classify_falls_back_to_unknown_for_other_variants() {
        // RustError carries a String but isn't an email-classified variant.
        let (code, status) = classify(&worker::Error::RustError("something".into()));
        assert_eq!(code, "E_UNKNOWN");
        assert_eq!(status, 502);
    }
}
