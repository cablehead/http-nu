use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

use tokio_util::sync::CancellationToken;

use nu_cli::{add_cli_context, gather_parent_env_vars};
use nu_cmd_lang::create_default_context;
use nu_command::add_shell_command_context;
use nu_engine::eval_block_with_early_return;
use nu_parser::parse;
use nu_plugin_engine::{GetPlugin, PluginDeclaration};
use nu_protocol::engine::Command;
use nu_protocol::format_cli_error;
use nu_protocol::{
    debugger::WithoutDebug,
    engine::{Closure, EngineState, Redirection, Stack, StateWorkingSet},
    OutDest, PipelineData, PluginIdentity, RegisteredPlugin, ShellError, Signals, Span, Type,
    Value,
};

use crate::commands::{
    HighlightCommand, HighlightLangCommand, HighlightThemeCommand, MdCommand, MjCommand,
    MjCompileCommand, MjRenderCommand, PrintCommand, ReverseProxyCommand, StaticCommand, ToSse,
};
use crate::logging::log_error;
use crate::stdlib::load_http_nu_stdlib;
use crate::Error;

#[derive(Clone)]
pub struct Engine {
    pub state: EngineState,
    pub closure: Option<Closure>,
    /// Cancellation token triggered on engine reload
    pub reload_token: CancellationToken,
}

impl Engine {
    pub fn new() -> Result<Self, Error> {
        let mut engine_state = create_default_context();

        engine_state = add_shell_command_context(engine_state);
        engine_state = add_cli_context(engine_state);
        engine_state = nu_cmd_extra::extra::add_extra_command_context(engine_state);

        load_http_nu_stdlib(&mut engine_state)?;

        let init_cwd = std::env::current_dir()?;
        gather_parent_env_vars(&mut engine_state, init_cwd.as_ref());

        Ok(Self {
            state: engine_state,
            closure: None,
            reload_token: CancellationToken::new(),
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

    /// Load a Nushell plugin from the given path
    pub fn load_plugin(&mut self, path: &Path) -> Result<(), Error> {
        // Canonicalize the path
        let path = path.canonicalize().map_err(|e| {
            Error::from(format!("Failed to canonicalize plugin path {path:?}: {e}"))
        })?;

        // Create the plugin identity
        let identity = PluginIdentity::new(&path, None).map_err(|_| {
            Error::from(format!(
                "Invalid plugin path {path:?}: must be named nu_plugin_*"
            ))
        })?;

        let mut working_set = StateWorkingSet::new(&self.state);

        // Add plugin to working set and get handle
        let plugin = nu_plugin_engine::add_plugin_to_working_set(&mut working_set, &identity)?;

        // Merge working set to make plugin available
        self.state.merge_delta(working_set.render())?;

        // Spawn the plugin to get its signatures
        let interface = plugin.clone().get_plugin(None)?;

        // Set plugin metadata
        plugin.set_metadata(Some(interface.get_metadata()?));

        // Add command declarations from plugin signatures
        let mut working_set = StateWorkingSet::new(&self.state);
        for signature in interface.get_signature()? {
            let decl = PluginDeclaration::new(plugin.clone(), signature);
            working_set.add_decl(Box::new(decl));
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
            return Err(Error::from(format_cli_error(
                &working_set,
                &shell_error,
                None,
            )));
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
            return Err(Error::from(format_cli_error(
                &working_set,
                &shell_error,
                None,
            )));
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
            Error::from(format_cli_error(&working_set, &err, None))
        })?;

        let closure = result
            .body
            .into_value(Span::unknown())
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_cli_error(&working_set, &err, None))
            })?
            .into_closure()
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_cli_error(&working_set, &err, None))
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

    /// Sets NU_LIB_DIRS const for module resolution
    pub fn set_lib_dirs(&mut self, paths: &[std::path::PathBuf]) -> Result<(), Error> {
        if paths.is_empty() {
            return Ok(());
        }
        let span = Span::unknown();
        let vals: Vec<Value> = paths
            .iter()
            .map(|p| Value::string(p.to_string_lossy(), span))
            .collect();

        let mut working_set = StateWorkingSet::new(&self.state);
        let var_id = working_set.add_variable(
            b"$NU_LIB_DIRS".into(),
            span,
            Type::List(Box::new(Type::String)),
            false,
        );
        working_set.set_variable_const_val(var_id, Value::list(vals, span));
        self.state.merge_delta(working_set.render())?;
        Ok(())
    }

    /// Evaluate a script string and return the result value
    pub fn eval(&self, script: &str) -> Result<Value, Error> {
        let mut working_set = StateWorkingSet::new(&self.state);
        let block = parse(&mut working_set, None, script.as_bytes(), false);

        if let Some(err) = working_set.parse_errors.first() {
            let shell_error = ShellError::GenericError {
                error: "Parse error".into(),
                msg: format!("{err:?}"),
                span: Some(err.span()),
                help: None,
                inner: vec![],
            };
            return Err(Error::from(format_cli_error(
                &working_set,
                &shell_error,
                None,
            )));
        }

        if let Some(err) = working_set.compile_errors.first() {
            let shell_error = ShellError::GenericError {
                error: format!("Compile error {err}"),
                msg: "".into(),
                span: None,
                help: None,
                inner: vec![],
            };
            return Err(Error::from(format_cli_error(
                &working_set,
                &shell_error,
                None,
            )));
        }

        // Clone engine state and merge the parsed block
        let mut engine_state = self.state.clone();
        engine_state.merge_delta(working_set.render())?;

        let mut stack = Stack::new();
        let result = eval_block_with_early_return::<WithoutDebug>(
            &engine_state,
            &mut stack,
            &block,
            PipelineData::empty(),
        )
        .map_err(|err| {
            let working_set = StateWorkingSet::new(&engine_state);
            Error::from(format_cli_error(&working_set, &err, None))
        })?;

        result.body.into_value(Span::unknown()).map_err(|err| {
            let working_set = StateWorkingSet::new(&engine_state);
            Error::from(format_cli_error(&working_set, &err, None))
        })
    }

    /// Run the parsed closure with input value and pipeline data
    pub fn run_closure(
        &self,
        input: Value,
        pipeline_data: PipelineData,
    ) -> Result<PipelineData, Error> {
        let closure = self.closure.as_ref().ok_or("Closure not parsed")?;

        let mut stack = Stack::new().captures_to_stack(closure.captures.clone());
        let mut stack =
            stack.push_redirection(Some(Redirection::Pipe(OutDest::PipeSeparate)), None);
        let block = self.state.get_block(closure.block_id);

        stack.add_var(
            block.signature.required_positional[0].var_id.unwrap(),
            input,
        );

        eval_block_with_early_return::<WithoutDebug>(&self.state, &mut stack, block, pipeline_data)
            .map(|exec_data| exec_data.body)
            .map_err(|err| {
                let working_set = StateWorkingSet::new(&self.state);
                Error::from(format_cli_error(&working_set, &err, None))
            })
    }

    /// Adds http-nu custom commands to the engine
    pub fn add_custom_commands(&mut self) -> Result<(), Error> {
        self.add_commands(vec![
            Box::new(ReverseProxyCommand::new()),
            Box::new(StaticCommand::new()),
            Box::new(ToSse {}),
            Box::new(MjCommand::new()),
            Box::new(MjCompileCommand::new()),
            Box::new(MjRenderCommand::new()),
            Box::new(HighlightCommand::new()),
            Box::new(HighlightThemeCommand::new()),
            Box::new(HighlightLangCommand::new()),
            Box::new(MdCommand::new()),
            Box::new(PrintCommand::new()),
        ])
    }

    /// Adds cross.stream store commands (.cat, .append, .cas, .head) to the engine
    #[cfg(feature = "cross-stream")]
    pub fn add_store_commands(&mut self, store: &xs::store::Store) -> Result<(), Error> {
        use xs::store::ZERO_CONTEXT;

        self.add_commands(vec![
            Box::new(xs::nu::commands::cat_stream_command::CatStreamCommand::new(
                store.clone(),
                ZERO_CONTEXT,
            )),
            Box::new(xs::nu::commands::append_command::AppendCommand::new(
                store.clone(),
                ZERO_CONTEXT,
                serde_json::json!({}),
            )),
            Box::new(xs::nu::commands::cas_command::CasCommand::new(
                store.clone(),
            )),
            Box::new(
                xs::nu::commands::head_stream_command::HeadStreamCommand::new(
                    store.clone(),
                    ZERO_CONTEXT,
                ),
            ),
            Box::new(xs::nu::commands::get_command::GetCommand::new(
                store.clone(),
            )),
            Box::new(xs::nu::commands::remove_command::RemoveCommand::new(
                store.clone(),
            )),
            Box::new(xs::nu::commands::scru128_command::Scru128Command::new()),
        ])
    }
}

/// Creates an engine from a script by cloning a base engine and parsing the closure.
/// On error, prints to stderr and emits JSON to stdout, returning None.
pub fn script_to_engine(base: &Engine, script: &str) -> Option<Engine> {
    let mut engine = base.clone();
    // Fresh cancellation token for this engine instance
    engine.reload_token = CancellationToken::new();

    if let Err(e) = engine.parse_closure(script) {
        log_error(&nu_utils::strip_ansi_string_likely(e.to_string()));
        return None;
    }

    Some(engine)
}
