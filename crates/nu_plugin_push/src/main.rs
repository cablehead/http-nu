// nu_plugin_push -- nushell plugin exposing Web Push.
//
// Commands:
//   push vapid generate                 -> { public_key, private_key_pem, private_key_b64url }
//   push send <payload>                 -> { endpoint, status, result, retry_after?, message? }
//   push encrypt <sub> <payload>        -> { curl, headers, url, body_hex, body_len }
//   push subscription parse <json>      -> { endpoint, keys: { p256dh, auth }, expiration_time? }
//   push subscription validate <sub>    -> { endpoint, reachable, vapid_accepted, status, message? }
//
// Env (read at runtime by send/validate):
//   VAPID_PRIVATE_KEY_PEM    -- private key as PEM (from fnox via mise)
//   VAPID_PRIVATE_KEY        -- fallback: URL-safe b64 of 32-byte scalar
//   VAPID_SUBJECT            -- mailto: or https: URL claim (Apple requires it)

use nu_plugin::{
    serve_plugin, EngineInterface, EvaluatedCall, MsgPackSerializer, Plugin, PluginCommand,
    SimplePluginCommand,
};
use nu_protocol::{LabeledError, Record, Signature, Span, SyntaxShape, Type, Value};

use nu_plugin_push::send::{DryRun, SendOpts, SendResult, ValidateResult};
use nu_plugin_push::subscription::PushSubscription;
use nu_plugin_push::vapid::GeneratedKeypair;
use nu_plugin_push::{send, subscription, vapid};

struct PushPlugin;

impl Plugin for PushPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(VapidGenerateCommand),
            Box::new(SendCommand),
            Box::new(EncryptCommand),
            Box::new(SubscriptionParseCommand),
            Box::new(SubscriptionValidateCommand),
        ]
    }
}

// =============================================================================
// push vapid generate
// =============================================================================

struct VapidGenerateCommand;

impl SimplePluginCommand for VapidGenerateCommand {
    type Plugin = PushPlugin;

    fn name(&self) -> &str {
        "push vapid generate"
    }

    fn signature(&self) -> Signature {
        Signature::build("push vapid generate").input_output_type(Type::Nothing, Type::Any)
    }

    fn description(&self) -> &str {
        "Generate a fresh P-256 VAPID keypair. Emits { public_key, private_key_pem, private_key_b64url } to stdout. No filesystem writes."
    }

    fn run(
        &self,
        _plugin: &PushPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let kp = vapid::generate().map_err(|e| label(format!("vapid generate: {e}")))?;
        Ok(keypair_to_value(kp, call.head))
    }
}

// =============================================================================
// push send
// =============================================================================

struct SendCommand;

impl SimplePluginCommand for SendCommand {
    type Plugin = PushPlugin;

    fn name(&self) -> &str {
        "push send"
    }

    fn signature(&self) -> Signature {
        Signature::build("push send")
            .required(
                "payload",
                SyntaxShape::String,
                "Payload string (JSON or text)",
            )
            .named(
                "ttl",
                SyntaxShape::Int,
                "TTL seconds (default 60). 0 = deliver immediately or drop.",
                Some('t'),
            )
            .named(
                "urgency",
                SyntaxShape::String,
                "very-low | low | normal | high",
                Some('u'),
            )
            .named(
                "topic",
                SyntaxShape::String,
                "Topic header for replace-on-receive semantics",
                None,
            )
            .input_output_type(Type::Any, Type::Any)
    }

    fn description(&self) -> &str {
        "Send a Web Push notification. Reads VAPID_PRIVATE_KEY_PEM (or VAPID_PRIVATE_KEY) + VAPID_SUBJECT from env. Input is a PushSubscription record or JSON string. Output: { endpoint, status, result, retry_after?, message? }."
    }

    fn run(
        &self,
        _plugin: &PushPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        let payload: String = call.req(0)?;
        let opts = send_opts_from_call(call)?;
        let body = payload.into_bytes();

        // Batch path: input is a list of subscriptions. Sequential for now;
        // true --parallel streaming is a separate task.
        if let Ok(items) = input.as_list() {
            let results: Vec<Value> = items
                .iter()
                .map(|item| match value_to_subscription(item) {
                    Ok(sub) => match send::send_one(&sub, body.clone(), &opts) {
                        Ok(r) => send_result_to_value(r, call.head),
                        Err(e) => send_error_value(&sub.endpoint, e.to_string(), call.head),
                    },
                    Err(e) => send_error_value("", format!("subscription: {e}"), call.head),
                })
                .collect();
            return Ok(Value::list(results, call.head));
        }

        // Single path
        let sub = value_to_subscription(input).map_err(|e| label(format!("subscription: {e}")))?;
        let result = send::send_one(&sub, body, &opts).map_err(|e| label(format!("send: {e}")))?;
        Ok(send_result_to_value(result, call.head))
    }
}

// =============================================================================
// push encrypt
// =============================================================================

struct EncryptCommand;

impl SimplePluginCommand for EncryptCommand {
    type Plugin = PushPlugin;

    fn name(&self) -> &str {
        "push encrypt"
    }

    fn signature(&self) -> Signature {
        Signature::build("push encrypt")
            .required(
                "subscription",
                SyntaxShape::Any,
                "PushSubscription record or JSON string",
            )
            .required("payload", SyntaxShape::String, "Payload to encrypt")
            .named(
                "ttl",
                SyntaxShape::Int,
                "TTL header value (default 60)",
                Some('t'),
            )
            .input_output_type(Type::Nothing, Type::Any)
    }

    fn description(&self) -> &str {
        "Encrypt + VAPID-sign a payload for a subscription without sending. Emits a curl one-liner + headers + hex-encoded body so you can debug crypto + VAPID without burning real push attempts."
    }

    fn run(
        &self,
        _plugin: &PushPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let sub_val: Value = call.req(0)?;
        let payload: String = call.req(1)?;
        let opts = send_opts_from_call(call)?;

        let sub =
            value_to_subscription(&sub_val).map_err(|e| label(format!("subscription: {e}")))?;

        let req = send::build_request(&sub, payload.into_bytes(), &opts)
            .map_err(|e| label(format!("build: {e}")))?;
        let dry = send::format_dry_run(&req);

        Ok(dry_run_to_value(dry, call.head))
    }
}

// =============================================================================
// push subscription parse
// =============================================================================

struct SubscriptionParseCommand;

impl SimplePluginCommand for SubscriptionParseCommand {
    type Plugin = PushPlugin;

    fn name(&self) -> &str {
        "push subscription parse"
    }

    fn signature(&self) -> Signature {
        Signature::build("push subscription parse")
            .required("json", SyntaxShape::String, "PushSubscription JSON")
            .input_output_type(Type::Nothing, Type::Any)
    }

    fn description(&self) -> &str {
        "Parse and validate a PushSubscription JSON string. Returns a typed record or a structured error."
    }

    fn run(
        &self,
        _plugin: &PushPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let json: String = call.req(0)?;
        let sub = PushSubscription::parse_json(&json).map_err(|e| label(format!("parse: {e}")))?;
        // Also exercise crypto-part decoding so we catch malformed keys early.
        sub.into_crypto_parts()
            .map_err(|e| label(format!("invalid keys: {e}")))?;
        Ok(subscription_to_value(&sub, call.head))
    }
}

// =============================================================================
// push subscription validate
// =============================================================================

struct SubscriptionValidateCommand;

impl SimplePluginCommand for SubscriptionValidateCommand {
    type Plugin = PushPlugin;

    fn name(&self) -> &str {
        "push subscription validate"
    }

    fn signature(&self) -> Signature {
        Signature::build("push subscription validate")
            .required(
                "subscription",
                SyntaxShape::Any,
                "PushSubscription record or JSON string",
            )
            .input_output_type(Type::Any, Type::Any)
    }

    fn description(&self) -> &str {
        "Send a TTL:0 empty-body push to verify the endpoint is reachable and our VAPID signature is accepted. No user-visible notification. Returns { endpoint, reachable, vapid_accepted, status, message? }."
    }

    fn run(
        &self,
        _plugin: &PushPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        // Accept either positional or piped subscription.
        let sub_val: Value = if let Ok(v) = call.req::<Value>(0) {
            v
        } else {
            input.clone()
        };
        let sub =
            value_to_subscription(&sub_val).map_err(|e| label(format!("subscription: {e}")))?;

        let res = send::validate(&sub).map_err(|e| label(format!("validate: {e}")))?;
        Ok(validate_result_to_value(res, call.head))
    }
}

// =============================================================================
// helpers: nu Value <-> typed structs
// =============================================================================

fn send_opts_from_call(call: &EvaluatedCall) -> Result<SendOpts, LabeledError> {
    let mut opts = SendOpts::default();
    if let Some(ttl) = call.get_flag::<i64>("ttl")? {
        opts.ttl_secs = ttl.max(0) as u64;
    }
    if let Some(u) = call.get_flag::<String>("urgency")? {
        opts.urgency = Some(u);
    }
    if let Some(t) = call.get_flag::<String>("topic")? {
        opts.topic = Some(t);
    }
    Ok(opts)
}

fn value_to_subscription(v: &Value) -> Result<PushSubscription, String> {
    if let Ok(s) = v.as_str() {
        return PushSubscription::parse_json(s).map_err(|e| e.to_string());
    }
    if let Ok(rec) = v.as_record() {
        let endpoint = require_str_rec(rec, "endpoint")?;
        let keys_val = rec.get("keys").ok_or("missing field: keys")?;
        let keys = keys_val.as_record().map_err(|e| format!("keys: {e}"))?;
        let p256dh = require_str_rec(keys, "p256dh")?;
        let auth = require_str_rec(keys, "auth")?;
        let expiration_time = rec
            .get("expirationTime")
            .or_else(|| rec.get("expiration_time"))
            .and_then(|v| v.as_int().ok())
            .map(|i| i as u64);
        return Ok(PushSubscription {
            endpoint,
            keys: subscription::SubscriptionKeys { p256dh, auth },
            expiration_time,
        });
    }
    Err("expected PushSubscription record or JSON string".into())
}

fn require_str_rec(rec: &Record, key: &str) -> Result<String, String> {
    rec.get(key)
        .and_then(|v| v.as_str().ok())
        .map(|s| s.to_owned())
        .ok_or_else(|| format!("missing required string field: {key}"))
}

fn keypair_to_value(kp: GeneratedKeypair, span: Span) -> Value {
    let mut r = Record::new();
    r.push("public_key", Value::string(kp.public_key_b64url, span));
    r.push("private_key_pem", Value::string(kp.private_key_pem, span));
    r.push(
        "private_key_b64url",
        Value::string(kp.private_key_b64url, span),
    );
    Value::record(r, span)
}

fn send_result_to_value(r: SendResult, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("endpoint", Value::string(r.endpoint, span));
    rec.push("status", Value::int(r.status as i64, span));
    rec.push("result", Value::string(r.outcome.as_str(), span));
    if let Some(s) = r.retry_after_secs {
        rec.push("retry_after", Value::int(s as i64, span));
    } else {
        rec.push("retry_after", Value::nothing(span));
    }
    if let Some(m) = r.message {
        rec.push("message", Value::string(m, span));
    } else {
        rec.push("message", Value::nothing(span));
    }
    Value::record(rec, span)
}

fn send_error_value(endpoint: &str, msg: String, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("endpoint", Value::string(endpoint, span));
    rec.push("status", Value::int(0, span));
    rec.push("result", Value::string("error", span));
    rec.push("retry_after", Value::nothing(span));
    rec.push("message", Value::string(msg, span));
    Value::record(rec, span)
}

fn validate_result_to_value(r: ValidateResult, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("endpoint", Value::string(r.endpoint, span));
    rec.push("reachable", Value::bool(r.reachable, span));
    rec.push("vapid_accepted", Value::bool(r.vapid_accepted, span));
    rec.push("status", Value::int(r.status as i64, span));
    if let Some(m) = r.message {
        rec.push("message", Value::string(m, span));
    } else {
        rec.push("message", Value::nothing(span));
    }
    Value::record(rec, span)
}

fn subscription_to_value(s: &PushSubscription, span: Span) -> Value {
    let mut keys = Record::new();
    keys.push("p256dh", Value::string(s.keys.p256dh.clone(), span));
    keys.push("auth", Value::string(s.keys.auth.clone(), span));

    let mut rec = Record::new();
    rec.push("endpoint", Value::string(s.endpoint.clone(), span));
    rec.push("keys", Value::record(keys, span));
    if let Some(et) = s.expiration_time {
        rec.push("expiration_time", Value::int(et as i64, span));
    } else {
        rec.push("expiration_time", Value::nothing(span));
    }
    Value::record(rec, span)
}

fn dry_run_to_value(d: DryRun, span: Span) -> Value {
    let mut hdrs = Record::new();
    for (k, v) in d.headers {
        hdrs.push(k, Value::string(v, span));
    }
    let mut rec = Record::new();
    rec.push("curl", Value::string(d.curl, span));
    rec.push("url", Value::string(d.url, span));
    rec.push("headers", Value::record(hdrs, span));
    rec.push("body_hex", Value::string(d.body_hex, span));
    rec.push("body_len", Value::int(d.body_len as i64, span));
    Value::record(rec, span)
}

fn label(msg: impl Into<String>) -> LabeledError {
    LabeledError::new(msg.into())
}

fn main() {
    serve_plugin(&PushPlugin, MsgPackSerializer)
}
