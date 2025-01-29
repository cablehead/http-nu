#[cfg(test)]
mod tests {
    use crate::{Engine, Request};

    #[test]
    fn test_engine_eval() {
        let mut engine = Engine::new().unwrap();

        // First parse the closure
        engine
            .parse_closure(r#"{|request| "hello world" }"#)
            .unwrap();

        let request = Request {
            method: "GET".into(),
            uri: "/".into(),
            path: "/".into(),
            headers: Default::default(),
            query: Default::default(),
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
}
