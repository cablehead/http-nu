// nu_plugin_email -- nushell plugin that POSTs structured email requests to
// our own Cloudflare Worker (`cf_email_worker`).
//
// Commands:
//   email send                   -- POST to worker /send (single record or list)
//   email send --dry-run         -- emit curl one-liner + JSON body, no POST
//   email config show            -- print env-var state (masking secrets)
//
// Env (read at runtime):
//   CF_EMAIL_WORKER_URL    -- e.g. https://email.<acct>.workers.dev
//   CF_EMAIL_AUTH_TOKEN    -- Bearer token; must match worker's secret

use nu_plugin::{
    serve_plugin, EngineInterface, EvaluatedCall, MsgPackSerializer, Plugin, PluginCommand,
    SimplePluginCommand,
};
use nu_protocol::{LabeledError, Record, Signature, Span, Type, Value};

use nu_plugin_email::outcome::Outcome;
use nu_plugin_email::send::{self, Config, EmailRequest, SendResult};

struct EmailPlugin;

impl Plugin for EmailPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![Box::new(SendCommand), Box::new(ConfigShowCommand)]
    }
}

// ============================================================================
// email send
// ============================================================================

struct SendCommand;

impl SimplePluginCommand for SendCommand {
    type Plugin = EmailPlugin;

    fn name(&self) -> &str {
        "email send"
    }

    fn signature(&self) -> Signature {
        Signature::build("email send")
            .switch(
                "dry-run",
                "Emit curl command + JSON body instead of sending",
                None,
            )
            .input_output_type(Type::Any, Type::Any)
    }

    fn description(&self) -> &str {
        "Send an email via the cf_email_worker. Input is a record { to, from, subject, text, html?, reply_to?, request_ref? } or a list of such records. Output: a record per input with { result, request_ref?, message_id?, error_code?, message?, retry_after? }."
    }

    fn run(
        &self,
        _plugin: &EmailPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        let dry = call.has_flag("dry-run").unwrap_or(false);

        let cfg = match Config::from_env() {
            Ok(c) => c,
            Err(e) => return Err(label(format!("config: {e}"))),
        };

        // Batch path: list input. Sequential -- true --parallel streaming is
        // a separate task in docs/email-native/TASKS.md (#18-equivalent).
        if let Ok(items) = input.as_list() {
            let results: Vec<Value> = items
                .iter()
                .map(|item| process_one(item, &cfg, dry, call.head))
                .collect();
            return Ok(Value::list(results, call.head));
        }

        // Single path.
        Ok(process_one(input, &cfg, dry, call.head))
    }
}

fn process_one(input: &Value, cfg: &Config, dry: bool, span: Span) -> Value {
    let req = match record_to_email_request(input) {
        Ok(r) => r,
        Err(msg) => return error_value("E_BAD_REQUEST", msg, None, span),
    };

    if dry {
        return match send::dry_run(cfg, &req) {
            Ok(curl) => dry_run_value(curl, &req, span),
            Err(e) => error_value(
                "E_CLIENT_ERROR",
                e.to_string(),
                req.request_ref.clone(),
                span,
            ),
        };
    }

    match send::perform_send(cfg, &req) {
        Ok(result) => send_result_to_value(result, span),
        Err(e) => error_value("E_TRANSPORT", e.to_string(), req.request_ref.clone(), span),
    }
}

// ============================================================================
// email config show
// ============================================================================

struct ConfigShowCommand;

impl SimplePluginCommand for ConfigShowCommand {
    type Plugin = EmailPlugin;

    fn name(&self) -> &str {
        "email config show"
    }

    fn signature(&self) -> Signature {
        Signature::build("email config show").input_output_type(Type::Nothing, Type::Any)
    }

    fn description(&self) -> &str {
        "Print plugin env-var state. Masks secret values. Exits non-zero if any required env var is missing."
    }

    fn run(
        &self,
        _plugin: &EmailPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let url = std::env::var("CF_EMAIL_WORKER_URL").ok();
        let token = std::env::var("CF_EMAIL_AUTH_TOKEN").ok();

        let mut rec = Record::new();
        rec.push(
            "CF_EMAIL_WORKER_URL",
            match url.as_deref() {
                Some(v) => Value::string(v.to_string(), call.head),
                None => Value::nothing(call.head),
            },
        );
        rec.push(
            "CF_EMAIL_AUTH_TOKEN",
            match token.as_deref() {
                Some(v) => Value::string(send::mask_token(v), call.head),
                None => Value::nothing(call.head),
            },
        );

        if url.is_none() || token.is_none() {
            return Err(label(format!(
                "missing required env: {}{}",
                if url.is_none() {
                    "CF_EMAIL_WORKER_URL "
                } else {
                    ""
                },
                if token.is_none() {
                    "CF_EMAIL_AUTH_TOKEN"
                } else {
                    ""
                },
            )));
        }

        Ok(Value::record(rec, call.head))
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn record_to_email_request(value: &Value) -> Result<EmailRequest, String> {
    let rec = value
        .as_record()
        .map_err(|e| format!("expected record: {e}"))?;
    let req_str = |name: &str| -> Result<String, String> {
        rec.get(name)
            .and_then(|v| v.as_str().ok())
            .map(|s| s.to_owned())
            .ok_or_else(|| format!("missing required string field: {name}"))
    };
    let opt_str = |name: &str| -> Option<String> {
        rec.get(name)
            .and_then(|v| v.as_str().ok())
            .map(|s| s.to_owned())
    };
    Ok(EmailRequest {
        to: req_str("to")?,
        from: req_str("from")?,
        subject: req_str("subject")?,
        text: req_str("text")?,
        html: opt_str("html"),
        reply_to: opt_str("reply_to"),
        request_ref: opt_str("request_ref"),
    })
}

fn send_result_to_value(r: SendResult, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("result", Value::string(r.outcome.as_str(), span));
    if let Some(req_ref) = r.request_ref {
        rec.push("request_ref", Value::string(req_ref, span));
    }
    if let Some(mid) = r.message_id {
        rec.push("message_id", Value::string(mid, span));
    }
    if let Some(code) = r.error_code {
        rec.push("error_code", Value::string(code, span));
    }
    if let Some(msg) = r.message {
        rec.push("message", Value::string(msg, span));
    }
    if let Some(ra) = r.retry_after {
        rec.push("retry_after", Value::int(ra as i64, span));
    }
    Value::record(rec, span)
}

/// Result record for client-side / transport errors. Same shape as a worker
/// error path so downstream xs handlers can branch uniformly on `result` and
/// optionally inspect `error_code`.
fn error_value(
    error_code: &str,
    message: String,
    request_ref: Option<String>,
    span: Span,
) -> Value {
    let mut rec = Record::new();
    rec.push("result", Value::string(Outcome::Failed.as_str(), span));
    rec.push("error_code", Value::string(error_code.to_owned(), span));
    rec.push("message", Value::string(message, span));
    if let Some(r) = request_ref {
        rec.push("request_ref", Value::string(r, span));
    }
    Value::record(rec, span)
}

fn dry_run_value(curl: String, req: &EmailRequest, span: Span) -> Value {
    let body =
        serde_json::to_string_pretty(req).unwrap_or_else(|e| format!("<serialize error: {e}>"));
    let mut rec = Record::new();
    rec.push("dry_run", Value::bool(true, span));
    rec.push("curl", Value::string(curl, span));
    rec.push("body", Value::string(body, span));
    if let Some(r) = &req.request_ref {
        rec.push("request_ref", Value::string(r.clone(), span));
    }
    Value::record(rec, span)
}

fn label(msg: impl Into<String>) -> LabeledError {
    LabeledError::new(msg.into())
}

fn main() {
    serve_plugin(&EmailPlugin, MsgPackSerializer)
}
