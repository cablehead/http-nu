// Result-record outcome enum. Mirrors the Cloudflare Email Service error
// codes so .nu handlers can branch cleanly on `result` instead of parsing
// raw HTTP status / error strings.
//
// Reference: https://developers.cloudflare.com/email-service/platform/limits/

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    RateLimited,
    DailyQuotaExceeded,
    SenderNotVerified,
    RecipientNotAllowed,
    Failed,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Delivered => "delivered",
            Outcome::RateLimited => "rate_limited",
            Outcome::DailyQuotaExceeded => "daily_quota_exceeded",
            Outcome::SenderNotVerified => "sender_not_verified",
            Outcome::RecipientNotAllowed => "recipient_not_allowed",
            Outcome::Failed => "failed",
        }
    }

    /// Classify a CF error code from the worker's JSON response.
    pub fn from_error_code(code: &str) -> Self {
        match code {
            "E_RATE_LIMIT_EXCEEDED" => Outcome::RateLimited,
            "E_DAILY_LIMIT_EXCEEDED" => Outcome::DailyQuotaExceeded,
            "E_SENDER_NOT_VERIFIED" => Outcome::SenderNotVerified,
            "E_RECIPIENT_NOT_ALLOWED" => Outcome::RecipientNotAllowed,
            _ => Outcome::Failed,
        }
    }
}
