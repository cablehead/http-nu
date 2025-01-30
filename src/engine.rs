use nu_cmd_lang::create_default_context;
use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Closure, EngineState, Stack, StateWorkingSet},
    PipelineData, Record, Span, Value,
};

use crate::{Error, Request};

#[derive(Clone)]
pub struct Engine {
    pub state: EngineState,
    closure: Option<Closure>,
}

impl Engine {
    pub fn new() -> Result<Self, Error> {
        let engine_state = create_default_context();
        Ok(Self {
            state: engine_state,
            closure: None,
        })
    }

    pub fn parse_closure(&mut self, script: &str) -> Result<(), Error> {
        let mut working_set = StateWorkingSet::new(&self.state);
        let block = parse(&mut working_set, None, script.as_bytes(), false);

        if !working_set.parse_errors.is_empty() {
            return Err("Parse error".into());
        }

        self.state.merge_delta(working_set.render())?;

        let mut stack = Stack::new();
        let result = eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            &block,
            PipelineData::empty(),
        )?;

        self.closure = Some(result.into_value(Span::unknown())?.into_closure()?);
        Ok(())
    }

    pub fn eval(&self, request: Request) -> Result<PipelineData, Error> {
        let closure = self.closure.as_ref().ok_or("Closure not parsed")?;
        let mut stack = Stack::new();
        let block = self.state.get_block(closure.block_id);

        stack.add_var(
            block.signature.required_positional[0].var_id.unwrap(),
            request_to_value(&request, Span::unknown()),
        );

        Ok(eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            block,
            PipelineData::empty(),
        )?)
    }
}

pub fn request_to_value(request: &Request, span: Span) -> Value {
    let mut record = Record::new();

    record.push("proto", Value::string(request.proto.clone(), span));
    record.push("method", Value::string(request.method.to_string(), span));
    record.push("uri", Value::string(request.uri.to_string(), span));
    record.push("path", Value::string(request.path.clone(), span));

    if let Some(authority) = &request.authority {
        record.push("authority", Value::string(authority.clone(), span));
    }

    if let Some(remote_ip) = &request.remote_ip {
        record.push("remote_ip", Value::string(remote_ip.to_string(), span));
    }

    if let Some(remote_port) = &request.remote_port {
        record.push("remote_port", Value::int(*remote_port as i64, span));
    }

    // Convert headers to a record
    let mut headers_record = Record::new();
    for (key, value) in request.headers.iter() {
        headers_record.push(
            key.to_string(),
            Value::string(value.to_str().unwrap_or_default().to_string(), span),
        );
    }
    record.push("headers", Value::record(headers_record, span));

    // Convert query parameters to a record
    let mut query_record = Record::new();
    for (key, value) in &request.query {
        query_record.push(key.clone(), Value::string(value.clone(), span));
    }
    record.push("query", Value::record(query_record, span));

    Value::record(record, span)
}
