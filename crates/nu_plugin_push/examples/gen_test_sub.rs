// Emit a valid synthetic PushSubscription JSON for testing serve.nu without a
// real browser. Endpoint points at Mozilla autopush (won't actually deliver --
// fake test ID -- but the crypto keys are real P-256 so /subscribe accepts it
// and the lifecycle / projection / send paths can all be exercised.
//
//   cargo run --example gen_test_sub -p nu_plugin_push
//
// Pipe into curl:
//   curl -X POST http://localhost:8090/subscribe \
//     -H 'Content-Type: application/json' -H 'Origin: http://localhost:8090' \
//     -d "$(cargo run --example gen_test_sub -p nu_plugin_push 2>/dev/null)"

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
use web_push_native::p256::SecretKey;

fn main() {
    let mut rng = web_push_native::p256::elliptic_curve::rand_core::OsRng;
    let sk = SecretKey::random(&mut rng);
    let pk = sk.public_key().to_encoded_point(false).as_bytes().to_vec();

    let mut auth = [0u8; 16];
    use web_push_native::p256::elliptic_curve::rand_core::RngCore;
    web_push_native::p256::elliptic_curve::rand_core::OsRng.fill_bytes(&mut auth);

    let sub = serde_json::json!({
        "endpoint": format!(
            "https://updates.push.services.mozilla.com/wpush/v2/test-{:08x}",
            rand_id()
        ),
        "keys": {
            "p256dh": URL_SAFE_NO_PAD.encode(&pk),
            "auth": URL_SAFE_NO_PAD.encode(&auth),
        }
    });
    println!("{}", sub);
}

fn rand_id() -> u32 {
    use web_push_native::p256::elliptic_curve::rand_core::RngCore;
    web_push_native::p256::elliptic_curve::rand_core::OsRng.next_u32()
}
