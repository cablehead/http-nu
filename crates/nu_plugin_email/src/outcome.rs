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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_cf_codes() {
        assert_eq!(
            Outcome::from_error_code("E_RATE_LIMIT_EXCEEDED"),
            Outcome::RateLimited
        );
        assert_eq!(
            Outcome::from_error_code("E_DAILY_LIMIT_EXCEEDED"),
            Outcome::DailyQuotaExceeded
        );
        assert_eq!(
            Outcome::from_error_code("E_SENDER_NOT_VERIFIED"),
            Outcome::SenderNotVerified
        );
        assert_eq!(
            Outcome::from_error_code("E_RECIPIENT_NOT_ALLOWED"),
            Outcome::RecipientNotAllowed
        );
    }

    #[test]
    fn unknown_codes_fall_through_to_failed() {
        assert_eq!(Outcome::from_error_code("E_UNKNOWN"), Outcome::Failed);
        assert_eq!(
            Outcome::from_error_code("E_RECIPIENT_SUPPRESSED"),
            Outcome::Failed
        );
        assert_eq!(
            Outcome::from_error_code("E_INTERNAL_SERVER_ERROR"),
            Outcome::Failed
        );
        assert_eq!(Outcome::from_error_code(""), Outcome::Failed);
        assert_eq!(
            Outcome::from_error_code("anything not on the list"),
            Outcome::Failed
        );
    }

    #[test]
    fn as_str_is_stable_kebab_for_each_variant() {
        // The strings here are part of the public wire format: nu handlers
        // and xs consumers branch on the `result` field. Lock them in.
        assert_eq!(Outcome::Delivered.as_str(), "delivered");
        assert_eq!(Outcome::RateLimited.as_str(), "rate_limited");
        assert_eq!(Outcome::DailyQuotaExceeded.as_str(), "daily_quota_exceeded");
        assert_eq!(Outcome::SenderNotVerified.as_str(), "sender_not_verified");
        assert_eq!(
            Outcome::RecipientNotAllowed.as_str(),
            "recipient_not_allowed"
        );
        assert_eq!(Outcome::Failed.as_str(), "failed");
    }
}
