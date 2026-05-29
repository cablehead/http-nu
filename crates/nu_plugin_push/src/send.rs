// Build, sign, encrypt, POST, classify. The whole send pipeline.
//
// web-push-native produces a complete http::Request<Vec<u8>> with the right
// VAPID Authorization (per-endpoint `aud` derived from the endpoint URI),
// Content-Encoding: aes128gcm, and the encrypted body. We override the TTL
// header (web-push-native ties TTL to JWT validity which we don't want)
// and add optional Urgency / Topic. Then ureq POSTs it.

use std::time::Duration;

use http::header::HeaderValue;
use thiserror::Error;
use web_push_native::WebPushBuilder;

use crate::subscription::{PushSubscription, SubError};
use crate::vapid;

const DEFAULT_JWT_VALIDITY: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Debug, Error)]
pub enum SendError {
    #[error("subscription: {0}")]
    Sub(#[from] SubError),
    #[error("vapid env: {0}")]
    VapidEnv(String),
    #[error("encrypt/sign: {0}")]
    Build(String),
    #[error("invalid header value: {0}")]
    Header(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    Expired,
    PayloadTooLarge,
    RateLimited,
    InvalidVapid,
    PushServiceDown,
    Other,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Delivered => "delivered",
            Outcome::Expired => "expired",
            Outcome::PayloadTooLarge => "payload_too_large",
            Outcome::RateLimited => "rate_limited",
            Outcome::InvalidVapid => "invalid_vapid",
            Outcome::PushServiceDown => "push_service_down",
            Outcome::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendOpts {
    pub ttl_secs: u64,
    pub urgency: Option<String>,
    pub topic: Option<String>,
}

impl Default for SendOpts {
    fn default() -> Self {
        Self {
            ttl_secs: 60,
            urgency: None,
            topic: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendResult {
    pub endpoint: String,
    pub status: u16,
    pub outcome: Outcome,
    pub retry_after_secs: Option<u64>,
    pub message: Option<String>,
}

/// Build the encrypted + VAPID-signed POST request without sending.
/// Used by both `send` (which POSTs it) and `encrypt --dry-run` (which
/// formats it as curl).
pub fn build_request(
    sub: &PushSubscription,
    payload: Vec<u8>,
    opts: &SendOpts,
) -> Result<http::Request<Vec<u8>>, SendError> {
    let (endpoint, public_key, auth) = sub.into_crypto_parts()?;

    let keypair = vapid::keypair_from_env().map_err(SendError::VapidEnv)?;
    let subject = vapid::subject_from_env().map_err(SendError::VapidEnv)?;

    let builder = WebPushBuilder::new(endpoint, public_key, auth)
        .with_valid_duration(DEFAULT_JWT_VALIDITY)
        .with_vapid(&keypair, &subject);

    let mut req = builder
        .build(payload)
        .map_err(|e| SendError::Build(e.to_string()))?;

    // Override TTL -- web-push-native ties it to JWT validity (12h default),
    // but for actual push delivery we want caller-controlled (often 60s or 0).
    req.headers_mut()
        .insert("TTL", HeaderValue::from(opts.ttl_secs));

    if let Some(u) = &opts.urgency {
        req.headers_mut().insert(
            "Urgency",
            HeaderValue::from_str(u).map_err(|e| SendError::Header(e.to_string()))?,
        );
    }

    if let Some(t) = &opts.topic {
        req.headers_mut().insert(
            "Topic",
            HeaderValue::from_str(t).map_err(|e| SendError::Header(e.to_string()))?,
        );
    }

    Ok(req)
}

/// POST the built request via ureq and classify the response.
pub fn post(req: http::Request<Vec<u8>>) -> Result<SendResult, SendError> {
    let (parts, body) = req.into_parts();
    let url = parts.uri.to_string();

    let mut ureq_req = ureq::post(&url);
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            ureq_req = ureq_req.set(name.as_str(), v);
        }
    }

    let resp_result = ureq_req.send_bytes(&body);
    classify(&url, resp_result)
}

fn classify(
    endpoint: &str,
    resp_result: Result<ureq::Response, ureq::Error>,
) -> Result<SendResult, SendError> {
    match resp_result {
        Ok(resp) => {
            let status = resp.status();
            let outcome = match status {
                200..=299 => Outcome::Delivered,
                _ => Outcome::Other,
            };
            Ok(SendResult {
                endpoint: endpoint.to_string(),
                status,
                outcome,
                retry_after_secs: None,
                message: None,
            })
        }
        Err(ureq::Error::Status(status, resp)) => {
            let retry_after = resp
                .header("Retry-After")
                .and_then(|s| s.parse::<u64>().ok());
            let message = resp.into_string().ok().filter(|s| !s.is_empty());
            let outcome = match status {
                404 | 410 => Outcome::Expired,
                413 => Outcome::PayloadTooLarge,
                429 => Outcome::RateLimited,
                400 | 401 | 403 => Outcome::InvalidVapid,
                500..=599 => Outcome::PushServiceDown,
                _ => Outcome::Other,
            };
            Ok(SendResult {
                endpoint: endpoint.to_string(),
                status,
                outcome,
                retry_after_secs: retry_after,
                message,
            })
        }
        Err(ureq::Error::Transport(t)) => Ok(SendResult {
            endpoint: endpoint.to_string(),
            status: 0,
            outcome: Outcome::PushServiceDown,
            retry_after_secs: None,
            message: Some(t.to_string()),
        }),
    }
}

/// End-to-end send: build + post + classify.
pub fn send_one(
    sub: &PushSubscription,
    payload: Vec<u8>,
    opts: &SendOpts,
) -> Result<SendResult, SendError> {
    let req = build_request(sub, payload, opts)?;
    post(req)
}

/// Validate: TTL:0 empty-body push. Returns the typed reachability result.
pub fn validate(sub: &PushSubscription) -> Result<ValidateResult, SendError> {
    let opts = SendOpts {
        ttl_secs: 0,
        urgency: Some("very-low".into()),
        topic: None,
    };
    let res = send_one(sub, Vec::new(), &opts)?;
    Ok(ValidateResult::from(res))
}

#[derive(Debug, Clone)]
pub struct ValidateResult {
    pub endpoint: String,
    pub reachable: bool,
    pub vapid_accepted: bool,
    pub status: u16,
    pub message: Option<String>,
}

impl From<SendResult> for ValidateResult {
    fn from(r: SendResult) -> Self {
        // Reachable = we got an HTTP response (status != 0).
        // VAPID accepted = response wasn't 401/403/400 (InvalidVapid).
        // 410 still counts as "vapid accepted, endpoint dead".
        let reachable = r.status != 0;
        let vapid_accepted = r.outcome != Outcome::InvalidVapid && reachable;
        ValidateResult {
            endpoint: r.endpoint,
            reachable,
            vapid_accepted,
            status: r.status,
            message: r.message,
        }
    }
}

/// Format a built request as a curl one-liner + structured headers + body hex.
/// Body is binary so curl needs `--data-binary @file`; we emit hex so the
/// caller can write it to a file or feed it via shell.
pub fn format_dry_run(req: &http::Request<Vec<u8>>) -> DryRun {
    let url = req.uri().to_string();
    let mut headers = Vec::new();
    let mut curl_args = vec!["curl".to_string(), "-X".to_string(), "POST".to_string()];

    for (name, value) in req.headers().iter() {
        let v = value.to_str().unwrap_or("<binary>");
        headers.push((name.as_str().to_string(), v.to_string()));
        curl_args.push("-H".into());
        curl_args.push(format!("'{}: {}'", name.as_str(), v));
    }

    curl_args.push("--data-binary".into());
    curl_args.push("@body.bin".into());
    curl_args.push(format!("'{url}'"));

    let body_hex = req
        .body()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    DryRun {
        curl: curl_args.join(" "),
        headers,
        url,
        body_hex,
        body_len: req.body().len(),
    }
}

#[derive(Debug, Clone)]
pub struct DryRun {
    pub curl: String,
    pub headers: Vec<(String, String)>,
    pub url: String,
    pub body_hex: String,
    pub body_len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::{PushSubscription, SubscriptionKeys};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
    use web_push_native::p256::SecretKey;

    fn synthetic_subscription() -> PushSubscription {
        let sk = SecretKey::random(&mut web_push_native::p256::elliptic_curve::rand_core::OsRng);
        let pk_bytes = sk.public_key().to_encoded_point(false).as_bytes().to_vec();
        let mut auth = [0u8; 16];
        getrandom::getrandom(&mut auth).expect("auth bytes");
        PushSubscription {
            endpoint: "https://updates.push.services.mozilla.com/wpush/v2/fake-test-id".into(),
            keys: SubscriptionKeys {
                p256dh: URL_SAFE_NO_PAD.encode(&pk_bytes),
                auth: URL_SAFE_NO_PAD.encode(&auth),
            },
            expiration_time: None,
        }
    }

    #[test]
    fn build_request_emits_signed_encrypted_post() {
        // Fresh VAPID keypair in env.
        let kp = crate::vapid::generate().expect("vapid gen");
        std::env::set_var("VAPID_PRIVATE_KEY", &kp.private_key_b64url);
        std::env::set_var("VAPID_SUBJECT", "mailto:test@example.com");
        std::env::remove_var("VAPID_PRIVATE_KEY_PEM");

        let sub = synthetic_subscription();
        let req = build_request(
            &sub,
            b"hello world".to_vec(),
            &SendOpts {
                ttl_secs: 60,
                urgency: Some("normal".into()),
                topic: Some("greeting".into()),
            },
        )
        .expect("build");

        assert_eq!(req.method(), http::Method::POST);
        assert_eq!(req.uri().to_string(), sub.endpoint);

        // Required headers
        let auth = req
            .headers()
            .get("Authorization")
            .expect("Authorization header");
        let auth_str = auth.to_str().expect("auth utf8");
        assert!(auth_str.starts_with("vapid t="), "VAPID header format");
        assert!(auth_str.contains(", k="), "VAPID k= component");

        assert_eq!(
            req.headers()
                .get("Content-Encoding")
                .and_then(|v| v.to_str().ok()),
            Some("aes128gcm")
        );
        assert_eq!(
            req.headers().get("TTL").and_then(|v| v.to_str().ok()),
            Some("60"),
            "TTL override took effect"
        );
        assert_eq!(
            req.headers().get("Urgency").and_then(|v| v.to_str().ok()),
            Some("normal")
        );
        assert_eq!(
            req.headers().get("Topic").and_then(|v| v.to_str().ok()),
            Some("greeting")
        );

        // Body is encrypted -- can't be plain "hello world", and must be longer
        // than the plaintext (16-byte salt + 65-byte ephemeral key + ciphertext).
        assert!(req.body().len() > b"hello world".len() + 80);
        assert_ne!(req.body(), &b"hello world".to_vec());
    }

    #[test]
    fn dry_run_emits_curl_and_hex_body() {
        let kp = crate::vapid::generate().expect("vapid gen");
        std::env::set_var("VAPID_PRIVATE_KEY", &kp.private_key_b64url);
        std::env::set_var("VAPID_SUBJECT", "mailto:test@example.com");
        std::env::remove_var("VAPID_PRIVATE_KEY_PEM");

        let sub = synthetic_subscription();
        let req = build_request(&sub, b"x".to_vec(), &SendOpts::default()).expect("build");
        let dry = format_dry_run(&req);

        assert!(dry.curl.starts_with("curl -X POST"));
        assert!(dry.curl.contains(&sub.endpoint));
        // HTTP headers serialize lowercase, so the curl line has `authorization`.
        assert!(
            dry.curl.to_lowercase().contains("'authorization: vapid"),
            "curl: {}",
            dry.curl
        );
        assert_eq!(dry.url, sub.endpoint);
        assert!(!dry.body_hex.is_empty());
        assert_eq!(dry.body_hex.len(), dry.body_len * 2, "hex doubles length");
    }
}
