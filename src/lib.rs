pub mod engine;
pub mod handler;
pub mod listener;
pub mod to_sse;

pub use engine::Engine;
pub use listener::Listener;
pub use to_sse::ToSse;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[cfg(test)]
mod test_engine;
#[cfg(test)]
mod test_handler;
