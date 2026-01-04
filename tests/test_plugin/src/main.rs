use nu_plugin::{
    serve_plugin, EngineInterface, EvaluatedCall, MsgPackSerializer, Plugin, PluginCommand,
    SimplePluginCommand,
};
use nu_protocol::{LabeledError, Signature, Span, Value};

struct TestPlugin;

impl Plugin for TestPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![Box::new(TestCommand)]
    }
}

struct TestCommand;

impl SimplePluginCommand for TestCommand {
    type Plugin = TestPlugin;

    fn name(&self) -> &str {
        "test-plugin-cmd"
    }

    fn signature(&self) -> Signature {
        Signature::build("test-plugin-cmd")
            .input_output_type(nu_protocol::Type::Any, nu_protocol::Type::String)
    }

    fn description(&self) -> &str {
        "A test command from the test plugin"
    }

    fn run(
        &self,
        _plugin: &TestPlugin,
        _engine: &EngineInterface,
        _call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        Ok(Value::string("PLUGIN_WORKS", Span::unknown()))
    }
}

fn main() {
    // Simulate slow plugin startup to verify plugin process is shared across requests
    std::thread::sleep(std::time::Duration::from_millis(100));
    serve_plugin(&TestPlugin, MsgPackSerializer)
}
