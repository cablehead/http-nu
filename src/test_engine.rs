use nu_protocol::{PipelineData, Value};

use crate::Engine;

fn eval_engine() -> Engine {
    let mut engine = Engine::new().unwrap();
    engine.add_custom_commands().unwrap();
    engine
}

#[test]
fn test_engine_eval() {
    let mut engine = Engine::new().unwrap();
    engine
        .parse_closure(r#"{|request| "hello world" }"#)
        .unwrap();

    let test_value = Value::test_string("hello world");
    let result = engine
        .run_closure(test_value, PipelineData::empty())
        .unwrap();

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

#[test]
fn test_mj_compile_inline() {
    let engine = eval_engine();
    let result = engine
        .eval(r#".mj compile --inline "Hello, {{ name }}""#)
        .unwrap();
    assert_eq!(result.get_type().to_string(), "CompiledTemplate");
}

#[test]
fn test_mj_compile_inline_html_record() {
    let engine = eval_engine();
    let result = engine
        .eval(r#".mj compile --inline {__html: "Hello, {{ name }}"}"#)
        .unwrap();
    assert_eq!(result.get_type().to_string(), "CompiledTemplate");
}

#[test]
fn test_mj_compile_syntax_error() {
    let engine = eval_engine();
    let result = engine.eval(r#".mj compile --inline "Hello, {{ name""#);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("compile error"));
}

#[test]
fn test_mj_compile_no_template() {
    let engine = eval_engine();
    let result = engine.eval(r#".mj compile"#);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No template specified"));
}

#[test]
fn test_mj_render() {
    let engine = eval_engine();
    let result = engine
        .eval(
            r#"let tpl = (.mj compile --inline "Hello, {{ name }}"); {name: "World"} | .mj render $tpl"#,
        )
        .unwrap();
    assert_eq!(result.as_str().unwrap(), "Hello, World");
}

#[test]
fn test_mj_render_loop() {
    let engine = eval_engine();
    let result = engine
        .eval(
            r#"let tpl = (.mj compile --inline "{% for i in items %}{{ i }}{% endfor %}"); {items: [1, 2, 3]} | .mj render $tpl"#,
        )
        .unwrap();
    assert_eq!(result.as_str().unwrap(), "123");
}

#[test]
fn test_mj_render_missing_var() {
    let engine = eval_engine();
    // MiniJinja renders missing variables as empty by default
    let result = engine
        .eval(r#"let tpl = (.mj compile --inline "Hello, {{ name }}"); {} | .mj render $tpl"#)
        .unwrap();
    assert_eq!(result.as_str().unwrap(), "Hello, ");
}

#[test]
fn test_mj_compile_describe() {
    let engine = eval_engine();
    let result = engine
        .eval(r#".mj compile --inline "test template" | describe"#)
        .unwrap();
    assert_eq!(result.as_str().unwrap(), "CompiledTemplate");
}
