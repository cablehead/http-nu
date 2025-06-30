use nu_protocol::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body_type: ResponseBodyType,
}

#[derive(Clone, Debug)]
pub enum ResponseBodyType {
    Normal,
    Static {
        root: PathBuf,
        path: String,
    },
    ReverseProxy {
        target_url: String,
        headers: HashMap<String, String>,
        timeout: Duration,
        preserve_host: bool,
        strip_prefix: Option<String>,
        request_body: Vec<u8>,
    },
}

#[derive(Debug)]
pub enum ResponseTransport {
    Empty,
    Full(Vec<u8>),
    Stream(tokio::sync::mpsc::Receiver<Vec<u8>>),
}

pub fn value_to_json(value: &Value) -> serde_json::Value {
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
        _ => todo!(),
    }
}

pub fn value_to_bytes(value: Value) -> Vec<u8> {
    match value {
        Value::Nothing { .. } => Vec::new(),
        Value::String { val, .. } => val.into_bytes(),
        Value::Int { val, .. } => val.to_string().into_bytes(),
        Value::Float { val, .. } => val.to_string().into_bytes(),
        Value::Binary { val, .. } => val,
        Value::Bool { val, .. } => val.to_string().into_bytes(),

        // Both Lists and Records are encoded as JSON
        Value::List { .. } | Value::Record { .. } => serde_json::to_string(&value_to_json(&value))
            .unwrap_or_else(|_| String::new())
            .into_bytes(),

        _ => todo!("value_to_bytes: {:?}", value),
    }
}
