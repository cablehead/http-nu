#[cfg(test)]
mod tests {
    use super::*;
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
            .eval_closure("{ echo 'hello world' }".into(), request)
            .unwrap();

        assert!(result
            .into_value(nu_protocol::Span::test_data())
            .unwrap()
            .as_string()
            .unwrap()
            .contains("hello world"));
    }
}
