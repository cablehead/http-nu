use nu_cmd_lang::create_default_context;
use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{EngineState, Stack, StateWorkingSet},
    PipelineData, Span, Value, VarId,
};

pub struct Engine {
    pub state: EngineState,
}

impl Engine {
    pub fn new() -> Result<Self, crate::Error> {
        let engine_state = create_default_context();
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

        if !working_set.parse_errors.is_empty() {
            return Err("Parse error in closure".into());
        }

        let mut stack = Stack::new();

        // Create request record
        let req_record = Value::record(nu_protocol::Record::new(), Span::test_data());

        // Add request as $request variable
        stack.add_var(VarId::new(0), req_record);

        let result = eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            &block,
            PipelineData::empty(),
        )?;

        Ok(result)
    }
}
