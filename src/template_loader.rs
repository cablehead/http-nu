//! Minijinja template loader that reads through `crate::vfs::Vfs`.
//!
//! Replaces `minijinja::path_loader(base)` everywhere `.mj` / `.mj
//! compile` need to resolve `{% include 'x.html' %}` /
//! `{% extends 'y.html' %}` / direct `env.get_template(name)`.
//! Routing through Vfs means desktop reads from disk via `OsVfs` and
//! CF reads from the per-request `SnapshotVfs` -- no `std::fs` calls
//! leak out of the upstream callsite.
//!
//! Lives in its own file (not under `src/cf/`) so that the upstream
//! `src/commands.rs` call site stays a one-liner regardless of target;
//! every line we add to upstream files is a future merge-conflict cost
//! against cablehead/http-nu.

use std::path::PathBuf;

/// Build a minijinja loader closure that resolves `<base>/<name>` via
/// `crate::vfs::with_vfs`. ENOENT (`std::io::ErrorKind::NotFound`)
/// maps to `Ok(None)` so minijinja's "template not found" path takes
/// over; other errors surface as `minijinja::Error` (SyntaxError
/// category -- minijinja doesn't expose a more specific loader-error
/// kind for runtime I/O failures).
pub fn vfs(
    base: PathBuf,
) -> impl Fn(&str) -> Result<Option<String>, minijinja::Error> + Send + Sync + 'static {
    move |name: &str| {
        let p = base.join(name);
        match crate::vfs::read_to_string(&p) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(minijinja::Error::new(
                minijinja::ErrorKind::SyntaxError,
                format!("template loader: {e}"),
            )),
        }
    }
}
