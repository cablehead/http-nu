use nu_protocol::{PipelineData, Value};

use crate::Engine;

#[test]
fn test_engine_eval() {
    let mut engine = Engine::new().unwrap();
    engine
        .parse_closure(r#"{|request| "hello world" }"#)
        .unwrap();

    let test_value = Value::test_string("hello world");
    let result = engine.eval(test_value, PipelineData::empty()).unwrap();

    assert!(result
        .into_value(nu_protocol::Span::test_data())
        .unwrap()
        .as_str()
        .unwrap()
        .contains("hello world"));
}

#[test]
fn test_closure_no_args() {
    let mut engine = Engine::new().unwrap();

    // Try to parse a closure with no arguments
    let result = engine.parse_closure(r#"{|| "hello world" }"#);

    // Assert the error contains the expected message
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Closure must accept exactly one request argument, found 0"));
}
