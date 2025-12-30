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

#[test]
fn test_closure_captures_outer_variables() {
    let mut engine = Engine::new().unwrap();
    engine
        .parse_closure(r#"let x = "captured"; {|req| $x}"#)
        .unwrap();

    let result = engine
        .run_closure(Value::test_nothing(), PipelineData::empty())
        .unwrap();

    assert_eq!(
        result
            .into_value(nu_protocol::Span::test_data())
            .unwrap()
            .as_str()
            .unwrap(),
        "captured"
    );
}

#[test]
fn test_highlight_rust() {
    let engine = eval_engine();
    let result = engine.eval(r#""fn main() {}" | .highlight rust"#).unwrap();
    let html = result.as_str().unwrap();
    assert!(html.contains("span"));
    assert!(html.contains("source rust"));
}

#[test]
fn test_highlight_nushell() {
    let engine = eval_engine();
    let result = engine
        .eval(r#""{|req| $req.path}" | .highlight nu"#)
        .unwrap();
    let html = result.as_str().unwrap();
    assert!(html.contains("span"));
    assert!(html.contains("source nu"));
}

#[test]
fn test_highlight_theme_list() {
    let engine = eval_engine();
    let result = engine.eval(r#".highlight theme"#).unwrap();
    let themes = result.as_list().unwrap();
    assert!(!themes.is_empty());
    // Check for some known themes
    let theme_names: Vec<_> = themes.iter().filter_map(|v| v.as_str().ok()).collect();
    assert!(theme_names.contains(&"Dracula"));
    assert!(theme_names.contains(&"Monokai Extended"));
}

#[test]
fn test_highlight_theme_css() {
    let engine = eval_engine();
    let result = engine.eval(r#".highlight theme Dracula"#).unwrap();
    let css = result.as_str().unwrap();
    assert!(css.contains("color:"));
    assert!(css.contains("background-color:"));
}

#[test]
fn test_highlight_lang_list() {
    let engine = eval_engine();
    let result = engine.eval(r#".highlight lang"#).unwrap();
    let langs = result.as_list().unwrap();
    assert!(!langs.is_empty());
    // Check structure: each item should have name and extensions
    let first = langs.first().unwrap().as_record().unwrap();
    assert!(first.get("name").is_some());
    assert!(first.get("extensions").is_some());
    // Check Nushell is present with "nu" extension
    let has_nushell = langs.iter().any(|v| {
        let rec = v.as_record().unwrap();
        rec.get("name").unwrap().as_str().unwrap() == "Nushell"
    });
    assert!(has_nushell);
}
