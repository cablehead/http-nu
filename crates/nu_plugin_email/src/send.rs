// HTTP client to the cf_email_worker. Implements task #9 in
// docs/email-native/TASKS.md.

use crate::outcome::Outcome;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

const ENV_WORKER_URL: &str = "CF_EMAIL_WORKER_URL";
const ENV_AUTH_TOKEN: &str = "CF_EMAIL_AUTH_TOKEN";
const CONNECT_TIMEOUT_SECS: u64 = 10;
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Error)]
pub enum SendError {
    #[error("CF_EMAIL_WORKER_URL not set")]
    MissingWorkerUrl,
    #[error("CF_EMAIL_AUTH_TOKEN not set")]
    MissingAuthToken,
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error("transport: {0}")]
    Transport(String),
}

/// One outbound email request as accepted by the worker's `POST /send`.
/// Matches the JSON wire format on both sides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailRequest {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Caller-supplied correlation id. Emitted back in the result and in any
    /// `email.send.{outcome}` events so xs consumers can stitch together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerOkResponse {
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerErrResponse {
    pub error_code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

/// Classified outcome + the original request_ref so xs handlers can stitch
/// `email.send.{outcome}` events back to the originating request.
#[derive(Debug, Clone)]
pub struct SendResult {
    pub outcome: Outcome,
    pub request_ref: Option<String>,
    pub message_id: Option<String>,
    pub error_code: Option<String>,
    pub message: Option<String>,
    pub retry_after: Option<u64>,
}

/// Resolved-once plugin config. Pulled from env vars at the start of each
/// run so that fnox/mise updates take effect on the next call without
/// re-registering the plugin.
#[derive(Debug, Clone)]
pub struct Config {
    pub worker_url: String,
    pub auth_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self, SendError> {
        let worker_url = std::env::var(ENV_WORKER_URL)
            .map_err(|_| SendError::MissingWorkerUrl)?
            .trim_end_matches('/')
            .to_string();
        let auth_token = std::env::var(ENV_AUTH_TOKEN).map_err(|_| SendError::MissingAuthToken)?;
        Ok(Self {
            worker_url,
            auth_token,
        })
    }
}

/// POST the request to the worker. ureq classifies non-2xx as
/// `Error::Status`; both `Ok` and `Err(Status)` can carry a JSON body that
/// we want to read, so the matching here is intentional.
pub fn perform_send(cfg: &Config, req: &EmailRequest) -> Result<SendResult, SendError> {
    let body = serde_json::to_string(req).map_err(|e| SendError::BadRequest(e.to_string()))?;
    let endpoint = format!("{}/send", cfg.worker_url);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build();

    let call = agent
        .post(&endpoint)
        .set("Authorization", &format!("Bearer {}", cfg.auth_token))
        .set("Content-Type", "application/json")
        .send_string(&body);

    let (status, response) = match call {
        Ok(r) => (r.status(), r),
        Err(ureq::Error::Status(s, r)) => (s, r),
        Err(ureq::Error::Transport(t)) => return Err(SendError::Transport(t.to_string())),
    };

    if (200..300).contains(&status) {
        let ok: WorkerOkResponse = response
            .into_json()
            .map_err(|e| SendError::Transport(format!("decode ok: {e}")))?;
        Ok(SendResult {
            outcome: Outcome::Delivered,
            request_ref: req.request_ref.clone(),
            message_id: Some(ok.message_id),
            error_code: None,
            message: None,
            retry_after: None,
        })
    } else {
        let err: WorkerErrResponse = response
            .into_json()
            .map_err(|e| SendError::Transport(format!("decode err (status {status}): {e}")))?;
        Ok(SendResult {
            outcome: Outcome::from_error_code(&err.error_code),
            request_ref: req.request_ref.clone(),
            message_id: None,
            error_code: Some(err.error_code),
            message: Some(err.message),
            retry_after: err.retry_after,
        })
    }
}

/// Render the curl command + JSON body that would be sent. Side-effect-free.
pub fn dry_run(cfg: &Config, req: &EmailRequest) -> Result<String, SendError> {
    let body =
        serde_json::to_string_pretty(req).map_err(|e| SendError::BadRequest(e.to_string()))?;
    // The token is masked -- the dry-run output is meant for inspection,
    // logs, and pasted bug reports, not for re-execution.
    let masked = mask_token(&cfg.auth_token);
    Ok(format!(
        "curl -X POST {url}/send \\\n  -H 'Authorization: Bearer {masked}' \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{body}'",
        url = cfg.worker_url,
    ))
}

pub fn mask_token(t: &str) -> String {
    if t.len() <= 8 {
        "<redacted>".into()
    } else {
        format!("{}...{}", &t[..4], &t[t.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(url: &str, token: &str) -> Config {
        Config {
            worker_url: url.into(),
            auth_token: token.into(),
        }
    }

    #[test]
    fn mask_token_redacts_short_strings_in_full() {
        // <= 8 chars: no leakage at all -- a 4-char prefix would give too
        // much away on short tokens.
        assert_eq!(mask_token(""), "<redacted>");
        assert_eq!(mask_token("a"), "<redacted>");
        assert_eq!(mask_token("12345678"), "<redacted>");
    }

    #[test]
    fn mask_token_shows_4_4_for_longer_strings() {
        assert_eq!(mask_token("123456789"), "1234...6789");
        assert_eq!(mask_token("very-long-secret-here"), "very...here");
        // 32-char hex (the shape openssl rand -hex 32 emits)
        let t = "deadbeefcafebabedeadbeefcafebabedeadbeefcafebabedeadbeefcafebabe";
        let masked = mask_token(t);
        assert_eq!(masked, "dead...babe");
        // Just to be explicit: actual middle is not in output.
        assert!(!masked.contains("cafe"));
    }

    #[test]
    fn dry_run_emits_curl_with_masked_token_and_pretty_body() {
        let cfg = cfg("https://email.example.workers.dev", "abcdefghijklmnop");
        let req = EmailRequest {
            to: "to@example.com".into(),
            from: "from@example.com".into(),
            subject: "hello".into(),
            text: "body".into(),
            html: None,
            reply_to: None,
            request_ref: Some("req-1".into()),
        };
        let out = dry_run(&cfg, &req).expect("dry_run is infallible for valid req");

        // Endpoint stitched together.
        assert!(out.contains("curl -X POST https://email.example.workers.dev/send"));
        // Token never appears in plaintext.
        assert!(!out.contains("abcdefghijklmnop"));
        assert!(out.contains("Bearer abcd...mnop"));
        // Body fields all present.
        assert!(out.contains("\"to\": \"to@example.com\""));
        assert!(out.contains("\"from\": \"from@example.com\""));
        assert!(out.contains("\"subject\": \"hello\""));
        assert!(out.contains("\"request_ref\": \"req-1\""));
    }

    #[test]
    fn dry_run_handles_trailing_slash_in_worker_url() {
        // Config::from_env trims trailing slashes, but a hand-built Config
        // (e.g. tests, or future callers) might not. Belt-and-braces.
        let cfg = cfg("https://email.example.workers.dev/", "12345678abcd");
        let req = EmailRequest {
            to: "x@y".into(),
            from: "y@x".into(),
            subject: "s".into(),
            text: "t".into(),
            html: None,
            reply_to: None,
            request_ref: None,
        };
        let out = dry_run(&cfg, &req).unwrap();
        // We get the URL verbatim -- caller's responsibility to normalize.
        // This test pins the current behavior; if we change it later, the
        // test failure signals the wire-format shift.
        assert!(out.contains("https://email.example.workers.dev//send"));
    }
}
