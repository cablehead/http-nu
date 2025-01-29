use crate::{Error, Request};
use nu_cmd_lang::create_default_context;
use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Closure, EngineState, Stack, StateWorkingSet},
    PipelineData, Span,
};

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

        let request_value = nu_protocol::Value::record(request.into_record()?, Span::unknown());

        stack.add_var(
            block.signature.required_positional[0].var_id.unwrap(),
            request_value,
        );

        Ok(eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            block,
            PipelineData::empty(),
        )?)
    }
}

// Add trait to convert Request into nu_protocol Record
trait IntoRecord {
    fn into_record(self) -> Result<nu_protocol::Record, Error>;
}

impl IntoRecord for Request {
    fn into_record(self) -> Result<nu_protocol::Record, Error> {
        let mut record = nu_protocol::Record::new();
        record.push(
            "method",
            nu_protocol::Value::string(self.method, Span::unknown()),
        );
        record.push("uri", nu_protocol::Value::string(self.uri, Span::unknown()));
        record.push(
            "path",
            nu_protocol::Value::string(self.path, Span::unknown()),
        );
        // Add headers and query params...
        Ok(record)
    }
}
