// cf-spike: prove curated Nushell runs on wasm32-unknown-unknown,
// callable from JS. Engine setup mirrors http-nu's desktop init in
// src/engine.rs and nu-on-web's playground engine.

use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Stack, StateWorkingSet},
    PipelineData, Span,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Evaluate a Nushell script and return either the rendered value as a string
/// or an error message. Closure-style scripts work; this is enough to prove
/// the curated Nu surface evaluates end-to-end on wasm32.
#[wasm_bindgen(js_name = "evalNu")]
pub fn eval_nu(script: &str) -> Result<String, JsError> {
    let mut engine_state = nu_cmd_lang::create_default_context();
    engine_state = nu_command::add_shell_command_context(engine_state);
    engine_state = nu_cmd_extra::add_extra_command_context(engine_state);

    let block = {
        let mut ws = StateWorkingSet::new(&engine_state);
        let block = nu_parser::parse(&mut ws, None, script.as_bytes(), false);
        if !ws.parse_errors.is_empty() {
            return Err(JsError::new(&format!("parse error: {:?}", ws.parse_errors)));
        }
        engine_state
            .merge_delta(ws.render())
            .map_err(|e| JsError::new(&format!("merge error: {e:?}")))?;
        block
    };

    let mut stack = Stack::new();
    let pd = nu_engine::eval_block::<WithoutDebug>(
        &engine_state,
        &mut stack,
        &block,
        PipelineData::Empty,
    )
    .map_err(|e| JsError::new(&format!("eval error: {e:?}")))?;

    let value = pd
        .body
        .into_value(Span::unknown())
        .map_err(|e| JsError::new(&format!("into_value error: {e:?}")))?;

    Ok(value.to_expanded_string("\n", &engine_state.config))
}
