// Typed PushSubscription -- mirrors the JSON shape browsers emit from
// `pushManager.subscribe().toJSON()`. Accepts both record-shaped and
// JSON-string inputs from nushell.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use web_push_native::p256::PublicKey;
use web_push_native::Auth;

#[derive(Debug, Error)]
pub enum SubError {
    #[error("JSON parse: {0}")]
    Json(#[from] serde_json::Error),
    #[error("p256dh decode: {0}")]
    P256dh(String),
    #[error("auth decode: {0}")]
    AuthDecode(String),
    #[error("invalid p256dh public key: {0}")]
    InvalidP256dh(String),
    #[error("auth secret must be 16 bytes, got {0}")]
    AuthLen(usize),
    #[error("invalid endpoint URI: {0}")]
    Endpoint(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
    #[serde(default, rename = "expirationTime")]
    pub expiration_time: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

impl PushSubscription {
    pub fn parse_json(s: &str) -> Result<Self, SubError> {
        Ok(serde_json::from_str(s)?)
    }

    /// Decode the keys into the types web-push-native wants.
    pub fn into_crypto_parts(&self) -> Result<(http::Uri, PublicKey, Auth), SubError> {
        let endpoint: http::Uri = self
            .endpoint
            .parse()
            .map_err(|e: http::uri::InvalidUri| SubError::Endpoint(e.to_string()))?;

        let p256dh_bytes = URL_SAFE_NO_PAD
            .decode(&self.keys.p256dh)
            .map_err(|e| SubError::P256dh(e.to_string()))?;
        let public_key = PublicKey::from_sec1_bytes(&p256dh_bytes)
            .map_err(|e| SubError::InvalidP256dh(e.to_string()))?;

        let auth_bytes = URL_SAFE_NO_PAD
            .decode(&self.keys.auth)
            .map_err(|e| SubError::AuthDecode(e.to_string()))?;
        if auth_bytes.len() != 16 {
            return Err(SubError::AuthLen(auth_bytes.len()));
        }
        let auth = Auth::clone_from_slice(&auth_bytes);

        Ok((endpoint, public_key, auth))
    }
}
