use std::collections::HashMap;

use http::header::HeaderMap;
use http::{Method, Uri};

use crate::{Engine, Request};

#[test]
fn test_engine_eval() {
    let mut engine = Engine::new().unwrap();

    // First parse the closure
    engine
        .parse_closure(r#"{|request| "hello world" }"#)
        .unwrap();

    let request = Request {
        proto: "HTTP/1.1".into(),
        method: Method::GET,
        uri: "/".parse::<Uri>().unwrap(),
        path: "/".into(),
        authority: None,
        remote_ip: None,
        remote_port: None,
        headers: HeaderMap::new(),
        query: HashMap::new(),
    };

    // Then eval with request
    let result = engine.eval(request).unwrap();

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
