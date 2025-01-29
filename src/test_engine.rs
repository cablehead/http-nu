#[cfg(test)]
mod tests {
    use crate::{Engine, Request};

    #[test]
    fn test_engine_eval() {
        let engine = Engine::new().unwrap();

        let request = Request {
            method: "GET".into(),
            uri: "/".into(),
            path: "/".into(),
            headers: Default::default(),
            query: Default::default(),
        };

        let result = engine
            .eval_closure(r#"{|| "hello world" }"#.into(), request)
            .unwrap();

        assert!(result
            .into_value(nu_protocol::Span::test_data())
            .unwrap()
            .as_str()
            .unwrap()
            .contains("hello world"));
    }
}
