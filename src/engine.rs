use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{EngineState, Stack, StateWorkingSet},
    PipelineData, Span,
};

pub struct Engine {
    pub state: EngineState,
}

impl Engine {
    pub fn new() -> Result<Self, crate::Error> {
        let mut engine_state = nu_cli::create_default_context();
        engine_state = nu_cmd_lang::create_default_context(engine_state);
        Ok(Self {
            state: engine_state,
        })
    }

    pub fn eval_closure(
        &self,
        closure: String,
        request: crate::Request,
    ) -> Result<PipelineData, crate::Error> {
        let mut working_set = StateWorkingSet::new(&self.state);
        let block = parse(&mut working_set, None, closure.as_bytes(), false);

        // Handle parse errors
        if !working_set.parse_errors.is_empty() {
            return Err("Parse error in closure".into());
        }

        let mut stack = Stack::new();

        // Add request as variable
        stack.add_var(
            0,
            nu_protocol::Value::record(
                nu_protocol::record! {
                    "method" => nu_protocol::Value::string(request.method, Span::test_data()),
                    "uri" => nu_protocol::Value::string(request.uri, Span::test_data()),
                    "path" => nu_protocol::Value::string(request.path, Span::test_data()),
                },
                Span::test_data(),
            ),
        );

        let result = eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            &block,
            PipelineData::empty(),
        )?;

        Ok(result)
    }

    pub fn register_response_command(&mut self) -> Result<(), crate::Error> {
        // TODO: Implement custom response command registration
        Ok(())
    }
}
