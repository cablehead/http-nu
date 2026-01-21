use nu_protocol::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged)]
pub enum HeaderValue {
    Single(String),
    Multiple(Vec<String>),
}

/// HTTP response metadata extracted from pipeline metadata (`http.response`)
#[derive(Clone, Debug, Default)]
pub struct HttpResponseMeta {
    pub status: Option<u16>,
    pub headers: HashMap<String, HeaderValue>,
}

/// Special response types that bypass normal body handling
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, HeaderValue>,
    pub body_type: ResponseBodyType,
}

#[derive(Clone, Debug)]
pub enum ResponseBodyType {
    Normal,
    Static {
        root: PathBuf,
        path: String,
        fallback: Option<String>,
    },
    ReverseProxy {
        target_url: String,
        headers: HashMap<String, HeaderValue>,
        preserve_host: bool,
        strip_prefix: Option<String>,
        request_body: Vec<u8>,
        query: Option<HashMap<String, String>>,
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

/// Extract HTTP response metadata from pipeline metadata's `http.response` field
pub fn extract_http_response_meta(
    metadata: Option<&nu_protocol::PipelineMetadata>,
) -> HttpResponseMeta {
    let Some(meta) = metadata else {
        return HttpResponseMeta::default();
    };

    let Some(http_response) = meta.custom.get("http.response") else {
        return HttpResponseMeta::default();
    };

    let Ok(record) = http_response.as_record() else {
        return HttpResponseMeta::default();
    };

    let status = record
        .get("status")
        .and_then(|v| v.as_int().ok())
        .map(|v| v as u16);

    let headers = record
        .get("headers")
        .and_then(|v| v.as_record().ok())
        .map(|headers_record| {
            let mut map = HashMap::new();
            for (k, v) in headers_record.iter() {
                let header_value = match v {
                    Value::String { val, .. } => HeaderValue::Single(val.clone()),
                    Value::List { vals, .. } => {
                        let strings: Vec<String> = vals
                            .iter()
                            .filter_map(|v| v.as_str().ok())
                            .map(|s| s.to_string())
                            .collect();
                        HeaderValue::Multiple(strings)
                    }
                    _ => continue,
                };
                map.insert(k.clone(), header_value);
            }
            map
        })
        .unwrap_or_default();

    HttpResponseMeta { status, headers }
}

pub fn value_to_bytes(value: Value) -> Vec<u8> {
    match value {
        Value::Nothing { .. } => Vec::new(),
        Value::String { val, .. } => val.into_bytes(),
        Value::Int { val, .. } => val.to_string().into_bytes(),
        Value::Float { val, .. } => val.to_string().into_bytes(),
        Value::Binary { val, .. } => val,
        Value::Bool { val, .. } => val.to_string().into_bytes(),

        // Records with __html field are unwrapped to HTML string
        Value::Record { val, .. } if val.get("__html").is_some() => match val.get("__html") {
            Some(Value::String { val, .. }) => val.clone().into_bytes(),
            _ => Vec::new(),
        },

        // Both Lists and Records are encoded as JSON
        Value::List { .. } | Value::Record { .. } => serde_json::to_string(&value_to_json(&value))
            .unwrap_or_else(|_| String::new())
            .into_bytes(),

        _ => todo!("value_to_bytes: {:?}", value),
    }
}
