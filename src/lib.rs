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
    pub method: String,
    pub uri: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
}
