//! Nu shadow commands for the CF target.
//!
//! Layout mirrors `nu-command/src/<category>/<command>.rs`. Each shadow
//! lives in the same relative path as the stock command we're shadowing
//! so a diff against an upstream Nu update is file-for-file. Example:
//!
//!   nu-command/src/filesystem/ls.rs      ->  src/cf/commands/filesystem/ls.rs
//!   nu-command/src/path/exists.rs        ->  src/cf/commands/path/exists.rs
//!   nu-command/src/platform/sleep.rs     ->  src/cf/commands/platform/sleep.rs
//!
//! When Nu adds or restructures a stock command we want to shadow, add
//! or move the equivalent file here in the same relative path. Process
//! discipline; the compiler doesn't enforce it.
//!
//! All filesystem shadows route through `crate::cf::vfs::Vfs`. Stock
//! `date now` / `format date` / `random integer` are NOT shadowed --
//! they come from `nu-command` with the `js` feature enabled
//! (Cargo.toml `cloudflare` feature).

mod shared;

pub mod filesystem;
pub mod path;
pub mod platform;

pub use filesystem::{VfsCp, VfsGlob, VfsLs, VfsMkdir, VfsMv, VfsOpen, VfsRm, VfsSave};
pub use path::{VfsPathExists, VfsPathSelf};
pub use platform::Sleep;
