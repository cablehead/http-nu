pub mod commands;
pub mod compression;
pub mod engine;
pub mod handler;
pub mod listener;
pub mod request;
pub mod response;
pub mod worker;

#[cfg(test)]
mod test_engine;
#[cfg(test)]
mod test_handler;

pub use engine::Engine;
pub use listener::Listener;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
