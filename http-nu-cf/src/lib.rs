// http-nu-cf — minimal Cloudflare Workers entrypoint that:
//   1. Receives a fetch event
//   2. Builds a Nu `$req` record from the Request
//   3. Runs a hardcoded closure { |req| ... } against it
//   4. Returns the value as a Response
//
// This is the smallest possible "http-nu on Workers" — no streaming, no
// custom commands beyond nu-command/nu-cmd-extra defaults, no xs. Each
// of those is a follow-up.

use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Closure, EngineState, Stack, StateWorkingSet},
    PipelineData, Span, Value,
};
use worker::*;

// Hardcoded handler — eventually loaded from R2 / xs / KV.
const HANDLER_SCRIPT: &str = r#"
{|req| $"hello: ($req.method) ($req.path)"}
"#;

#[event(fetch)]
async fn fetch(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    match handle(&req) {
        Ok(body) => Response::ok(body),
        Err(err) => Response::error(err, 500),
    }
}

fn handle(req: &Request) -> std::result::Result<String, String> {
    let mut engine_state = nu_cmd_lang::create_default_context();
    engine_state = nu_command::add_shell_command_context(engine_state);
    engine_state = nu_cmd_extra::add_extra_command_context(engine_state);

    let closure = parse_closure(&mut engine_state, HANDLER_SCRIPT)?;
    let req_value = request_to_value(req);
    let pd = call_closure(&engine_state, &closure, req_value)?;

    let value = pd
        .body
        .into_value(Span::unknown())
        .map_err(|e| format!("into_value: {e:?}"))?;
    Ok(value.to_expanded_string("\n", &engine_state.config))
}

/// Parse a script that's expected to evaluate to a single closure value,
/// merge the parser delta into the engine, and return the closure.
fn parse_closure(engine_state: &mut EngineState, script: &str) -> std::result::Result<Closure, String> {
    let block = {
        let mut ws = StateWorkingSet::new(engine_state);
        let block = nu_parser::parse(&mut ws, None, script.as_bytes(), false);
        if !ws.parse_errors.is_empty() {
            return Err(format!("parse: {:?}", ws.parse_errors));
        }
        engine_state
            .merge_delta(ws.render())
            .map_err(|e| format!("merge: {e:?}"))?;
        block
    };

    let mut stack = Stack::new();
    let pd = nu_engine::eval_block::<WithoutDebug>(engine_state, &mut stack, &block, PipelineData::Empty)
        .map_err(|e| format!("eval: {e:?}"))?;
    let value = pd
        .body
        .into_value(Span::unknown())
        .map_err(|e| format!("into_value: {e:?}"))?;
    match value {
        Value::Closure { val, .. } => Ok(*val),
        other => Err(format!("expected closure, got {:?}", other.get_type())),
    }
}

fn call_closure(
    engine_state: &EngineState,
    closure: &Closure,
    arg: Value,
) -> std::result::Result<nu_protocol::PipelineExecutionData, String> {
    let block = engine_state.get_block(closure.block_id);
    let mut stack = Stack::new();

    // Bind closure captures.
    for (var_id, value) in &closure.captures {
        stack.add_var(*var_id, value.clone());
    }
    // Bind the single positional parameter (the closure's `req`).
    if let Some(sig) = &block.signature.required_positional.first() {
        stack.add_var(sig.var_id.expect("closure param missing var_id"), arg);
    }

    nu_engine::eval_block::<WithoutDebug>(engine_state, &mut stack, block, PipelineData::Empty)
        .map_err(|e| format!("call: {e:?}"))
}

/// Build a Nu record matching http-nu's `$req` shape (a subset for now).
fn request_to_value(req: &Request) -> Value {
    let span = Span::unknown();
    let url = req.url().ok();
    let path = url.as_ref().map(|u| u.path().to_string()).unwrap_or_default();
    let query = url
        .as_ref()
        .and_then(|u| u.query().map(|q| q.to_string()))
        .unwrap_or_default();

    Value::record(
        nu_protocol::record! {
            "method" => Value::string(req.method().to_string(), span),
            "path" => Value::string(path, span),
            "query" => Value::string(query, span),
        },
        span,
    )
}
