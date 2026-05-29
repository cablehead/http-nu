// VAPID keypair generation and loading.
//
// We store the raw 32-byte EC scalar (P-256) as URL-safe base64. That's the
// format web-push-native's ES256KeyPair::from_bytes accepts, and it's compact
// enough to drop into env vars without newline escaping headaches.
//
// PEM is also emitted on generate for tooling/humans, but the runtime path
// (VAPID_PRIVATE_KEY in env, read by `push send`) uses the b64url form.
//
// Public key: 65-byte uncompressed SEC1 (0x04 || X || Y) → URL-safe b64.
// That's what browsers want for `applicationServerKey`.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use thiserror::Error;
use web_push_native::jwt_simple::algorithms::ES256KeyPair;
use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
use web_push_native::p256::SecretKey;

#[derive(Debug, Error)]
pub enum VapidError {
    #[error("invalid VAPID private key bytes: {0}")]
    InvalidPrivateKey(String),
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("PEM encoding: {0}")]
    Pem(String),
}

pub struct GeneratedKeypair {
    /// 65-byte uncompressed SEC1 public key, URL-safe base64 (no padding).
    /// This is what the browser passes to `pushManager.subscribe` as
    /// `applicationServerKey`.
    pub public_key_b64url: String,
    /// Raw 32-byte private scalar, URL-safe base64 (no padding).
    /// Round-trips through `ES256KeyPair::from_bytes` for signing.
    pub private_key_b64url: String,
    /// PKCS#8 PEM form of the private key. Convenience for tools that prefer PEM.
    pub private_key_pem: String,
}

pub fn generate() -> Result<GeneratedKeypair, VapidError> {
    let kp = ES256KeyPair::generate();

    let private_bytes = kp.to_bytes();

    // Re-derive a p256::SecretKey from the same scalar so we can get the
    // uncompressed public key form that browsers need. ES256PublicKey's
    // to_bytes() returns compressed (33 bytes); we want uncompressed (65).
    let sk = SecretKey::from_slice(&private_bytes)
        .map_err(|e| VapidError::InvalidPrivateKey(e.to_string()))?;
    let pk = sk.public_key();
    let public_uncompressed = pk.to_encoded_point(false).as_bytes().to_vec();

    let private_key_pem = kp.to_pem().map_err(|e| VapidError::Pem(e.to_string()))?;

    Ok(GeneratedKeypair {
        public_key_b64url: URL_SAFE_NO_PAD.encode(&public_uncompressed),
        private_key_b64url: URL_SAFE_NO_PAD.encode(&private_bytes),
        private_key_pem,
    })
}

/// Load a keypair from the URL-safe base64 private scalar (env-friendly form).
pub fn load_keypair_b64url(b64: &str) -> Result<ES256KeyPair, VapidError> {
    let bytes = URL_SAFE_NO_PAD.decode(b64)?;
    ES256KeyPair::from_bytes(&bytes).map_err(|e| VapidError::InvalidPrivateKey(e.to_string()))
}

/// Load a keypair from PEM. Used when VAPID_PRIVATE_KEY_PEM env var is set
/// (fnox round-trips PEM fine via mise template injection).
pub fn load_keypair_pem(pem: &str) -> Result<ES256KeyPair, VapidError> {
    ES256KeyPair::from_pem(pem).map_err(|e| VapidError::InvalidPrivateKey(e.to_string()))
}

/// Read the VAPID keypair from the environment at runtime.
/// Prefers VAPID_PRIVATE_KEY_PEM, falls back to VAPID_PRIVATE_KEY (b64url).
pub fn keypair_from_env() -> Result<ES256KeyPair, String> {
    if let Ok(pem) = std::env::var("VAPID_PRIVATE_KEY_PEM") {
        return load_keypair_pem(&pem).map_err(|e| e.to_string());
    }
    if let Ok(b64) = std::env::var("VAPID_PRIVATE_KEY") {
        return load_keypair_b64url(&b64).map_err(|e| e.to_string());
    }
    Err("neither VAPID_PRIVATE_KEY_PEM nor VAPID_PRIVATE_KEY set in env".into())
}

pub fn subject_from_env() -> Result<String, String> {
    std::env::var("VAPID_SUBJECT").map_err(|_| {
        "VAPID_SUBJECT not set (must be mailto: or https: URL; Apple push requires it)".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_round_trippable_keypair() {
        let kp = generate().expect("generate");
        // Public key uncompressed is exactly 65 bytes
        let pub_bytes = URL_SAFE_NO_PAD
            .decode(&kp.public_key_b64url)
            .expect("decode pub");
        assert_eq!(pub_bytes.len(), 65, "uncompressed SEC1 = 65 bytes");
        assert_eq!(pub_bytes[0], 0x04, "uncompressed prefix");

        // Private key b64url is exactly 32 bytes
        let priv_bytes = URL_SAFE_NO_PAD
            .decode(&kp.private_key_b64url)
            .expect("decode priv");
        assert_eq!(priv_bytes.len(), 32, "P-256 scalar = 32 bytes");

        // Round-trip through ES256KeyPair
        load_keypair_b64url(&kp.private_key_b64url).expect("round-trip b64url");
        load_keypair_pem(&kp.private_key_pem).expect("round-trip pem");
    }
}
