//! Path shadow commands. Mirrors `nu-command/src/path/`.

mod exists;
mod self_;

pub use exists::VfsPathExists;
pub use self_::VfsPathSelf;
