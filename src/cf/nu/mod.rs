//! Nushell-side ports / shadows for the CF target.
//!
//! `src/cf/nu/` mirrors the upstream `nushell` repo structure:
//!
//!   nushell/crates/nu-command/<cat>/<name>.rs
//!     -> src/cf/nu/nu_command/<cat>/<name>.rs
//!
//! Today only `nu_command/` is populated -- CF-side shadows of stock
//! `nu-command` filesystem/path/platform commands that either don't
//! compile on wasm or do OS-level I/O that doesn't work in the
//! Workers runtime. If we ever port from `nu-protocol` or
//! `nu-engine`, they slot in as siblings of `nu_command/` (snake-case
//! mirror of the hyphenated upstream crate names).
//!
//! See `src/cf/nu/nu_command/CLAUDE.md` for the working rules.

pub mod nu_command;
