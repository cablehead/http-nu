use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod engine;
mod handler;
mod listener;

pub use engine::Engine;
pub use handler::handle;
pub use listener::Listener;

#[cfg(test)]
mod test_engine;
#[cfg(test)]
mod test_handler;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub proto: String,
    #[serde(with = "http_serde::method")]
    pub method: http::method::Method,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<std::net::IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(with = "http_serde::header_map")]
    pub headers: http::header::HeaderMap,
    #[serde(with = "http_serde::uri")]
    pub uri: http::Uri,
    pub path: String,
    pub query: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
}
