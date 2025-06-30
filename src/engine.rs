use nu_cli::{add_cli_context, gather_parent_env_vars};
use nu_cmd_lang::create_default_context;
use nu_command::add_shell_command_context;
use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_protocol::engine::Command;
use nu_protocol::format_shell_error;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Closure, EngineState, Redirection, Stack, StateWorkingSet},
    OutDest, PipelineData, ShellError, Signals, Span, Value,
};
use std::sync::{atomic::AtomicBool, Arc};

use crate::Error;

#[derive(Clone)]
pub struct Engine {
    pub state: EngineState,
    pub closure: Option<Closure>,
}

impl Engine {
    pub fn new() -> Result<Self, Error> {
        let mut engine_state = create_default_context();

        engine_state = add_shell_command_context(engine_state);
        engine_state = add_cli_context(engine_state);
        engine_state = nu_cmd_extra::extra::add_extra_command_context(engine_state);

        let init_cwd = std::env::current_dir()?;
        gather_parent_env_vars(&mut engine_state, init_cwd.as_ref());

        Ok(Self {
            state: engine_state,
            closure: None,
        })
    }

    pub fn add_commands(&mut self, commands: Vec<Box<dyn Command>>) -> Result<(), Error> {
        let mut working_set = StateWorkingSet::new(&self.state);
        for command in commands {
            working_set.add_decl(command);
        }
        self.state.merge_delta(working_set.render())?;
        Ok(())
    }

    pub fn parse_closure(&mut self, script: &str) -> Result<(), Error> {
        let mut working_set = StateWorkingSet::new(&self.state);
        let block = parse(&mut working_set, None, script.as_bytes(), false);

        // Handle parse errors
        if let Some(err) = working_set.parse_errors.first() {
            let shell_error = ShellError::GenericError {
                error: "Parse error".into(),
                msg: format!("{err:?}"),
                span: Some(err.span()),
                help: None,
                inner: vec![],
            };
            return Err(Error::from(format_shell_error(&working_set, &shell_error)));
        }

        // Handle compile errors
        if let Some(err) = working_set.compile_errors.first() {
            let shell_error = ShellError::GenericError {
                error: format!("Compile error {err}"),
                msg: "".into(),
                span: None,
                help: None,
                inner: vec![],
            };
            return Err(Error::from(format_shell_error(&working_set, &shell_error)));
        }

        self.state.merge_delta(working_set.render())?;

        let mut stack = Stack::new();
        let result = eval_block_with_early_return::<WithoutDebug>(
            &self.state,
            &mut stack,
            &block,
            PipelineData::empty(),
        )
        .map_err(|err| {
            let working_set = StateWorkingSet::new(&self.state);
            Error::from(format_shell_error(&working_set, &err))
        })?;

        let closure = result
            .into_value(Span::unknown())
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_shell_error(&working_set, &err))
            })?
            .into_closure()
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_shell_error(&working_set, &err))
            })?;

        // Verify closure accepts exactly one argument
        let block = self.state.get_block(closure.block_id);
        if block.signature.required_positional.len() != 1 {
            return Err(format!(
                "Closure must accept exactly one request argument, found {}",
                block.signature.required_positional.len()
            )
            .into());
        }

        self.state.merge_env(&mut stack)?;

        self.closure = Some(closure);
        Ok(())
    }

    /// Sets the interrupt signal for the engine
    pub fn set_signals(&mut self, interrupt: Arc<AtomicBool>) {
        self.state.set_signals(Signals::new(interrupt));
    }

    pub fn eval(&self, input: Value, pipeline_data: PipelineData) -> Result<PipelineData, Error> {
        let closure = self.closure.as_ref().ok_or("Closure not parsed")?;

        let mut stack = Stack::new();
        let mut stack =
            stack.push_redirection(Some(Redirection::Pipe(OutDest::PipeSeparate)), None);
        let block = self.state.get_block(closure.block_id);

        stack.add_var(
            block.signature.required_positional[0].var_id.unwrap(),
            input,
        );

        eval_block_with_early_return::<WithoutDebug>(&self.state, &mut stack, block, pipeline_data)
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_shell_error(&working_set, &err))
            })
    }
}
