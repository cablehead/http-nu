use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, Response};
use nu_protocol::Value;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<Response<BoxBody<Bytes, BoxError>>, BoxError>;

pub async fn handle<B>(engine: crate::Engine, req: Request<B>) -> HTTPResult
where
    B: hyper::body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    let (parts, _body) = req.into_parts();

    let request = crate::Request {
        method: parts.method.to_string(),
        uri: parts.uri.to_string(),
        path: parts.uri.path().to_string(),
        headers: parts
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect(),
        query: parts
            .uri
            .query()
            .map(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .into_owned()
                    .collect()
            })
            .unwrap_or_default(),
    };

    let result = tokio::task::spawn_blocking(move || engine.eval(request)).await??;
    let response_str = value_to_string(result.into_value(nu_protocol::Span::unknown())?);

    Ok(hyper::Response::builder().status(200).body(
        Full::new(response_str.into())
            .map_err(|never| match never {})
            .boxed(),
    )?)
}

use serde_json;

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Nothing { .. } => serde_json::Value::Null,
        Value::Bool { val, .. } => serde_json::Value::Bool(*val),
        Value::Int { val, .. } => serde_json::Value::Number((*val).into()),
        Value::Float { val, .. } => serde_json::Number::from_f64(*val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String { val, .. } => serde_json::Value::String(val.clone()),
        Value::List { vals, .. } => {
            serde_json::Value::Array(vals.iter().map(value_to_json).collect())
        }
        Value::Record { val, .. } => {
            let mut map = serde_json::Map::new();
            for (k, v) in val.iter() {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    }
}

fn value_to_string(value: Value) -> String {
    match value {
        Value::Nothing { .. } => String::new(),
        Value::String { val, .. } => val,
        Value::Int { val, .. } => val.to_string(),
        Value::Float { val, .. } => val.to_string(),
        Value::List { vals, .. } => {
            let items: Vec<String> = vals.iter().map(|v| value_to_string(v.clone())).collect();
            items.join("\n")
        }
        Value::Record { .. } => {
            serde_json::to_string(&value_to_json(&value)).unwrap_or_else(|_| String::new())
        }
        _ => String::new(),
    }
}
