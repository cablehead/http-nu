//! Conformance harness: run `cloudflare_shell::conformance`'s generic
//! `<F: FileSystem>` suite against a real `Workspace` and return the
//! result as a `worker::Response`.
//!
//! Wire this up from any `worker::Route` (or any handler that has the
//! caller's `SqlStorage` + `Bucket` in hand) to prove the DO SQLite +
//! R2 backend matches the trait contract. Useful for CI smoke tests
//! and for catching schema-compat regressions before they hit users.
//!
//! Example:
//!
//! ```ignore
//! match (request.path(), request.method()) {
//!     ("/_workspace/conformance", Method::Get) => {
//!         let sql = state.storage().sql();
//!         let r2  = env.bucket("WORKSPACE_FILES").ok();
//!         cloudflare_shell_workspace::run_conformance(sql, r2).await
//!     }
//!     _ => Response::error("not found", 404),
//! }
//! ```
//!
//! Output: `200 OK` + plain text `<n> passed` if every assertion
//! holds. On any assertion failure the panic escapes -- pair with
//! `console_error_panic_hook` so it returns `500` with a readable
//! backtrace.
//!
//! State: each fn assumes a fresh filesystem, so the harness calls
//! `wipe_root` between fns. It uses namespace `__conformance` to keep
//! its state segregated from the user's real workspace.

use worker::{Bucket, Response, Result, SqlStorage};

use cloudflare_shell::conformance as suite;

use crate::Workspace;

// `Workspace::new` rejects leading-underscore names (mirrors
// upstream's VALID_NAMESPACE = /^[a-zA-Z][a-zA-Z0-9_]*$/). Stays
// isolated from real data by name choice + `wipe_root` between fns.
const CONFORMANCE_NAMESPACE: &str = "conformance";

/// Drive every `cloudflare_shell::conformance` function against a
/// fresh `Workspace` under namespace `conformance`. Returns
/// `200 OK` + `"<n> passed"` on success; panics propagate (turn into
/// a `500` if `console_error_panic_hook` is installed).
pub async fn run_conformance(sql: SqlStorage, r2: Option<Bucket>) -> Result<Response> {
    let ws = Workspace::new(sql, r2, CONFORMANCE_NAMESPACE)?;

    // Each call: wipe state, then run the conformance fn. Panics
    // escape -- `console_error_panic_hook` turns them into 500s with
    // a readable backtrace, so the calling curl shows the first
    // failure. Tests listed alphabetically so the output is
    // predictable.
    wipe(&ws).await?;
    suite::cp_preserves_mime(&ws).await;

    wipe(&ws).await?;
    suite::eisdir_on_read_of_directory(&ws).await;

    wipe(&ws).await?;
    suite::eisdir_on_write_to_root(&ws).await;

    wipe(&ws).await?;
    suite::enoent_returns_ok_none(&ws).await;

    wipe(&ws).await?;
    suite::name_too_long(&ws).await;

    wipe(&ws).await?;
    suite::on_change_emits_create_then_update_then_delete(&ws, |ws, cb| ws.set_on_change(cb)).await;

    wipe(&ws).await?;
    suite::rm_recursive(&ws).await;

    wipe(&ws).await?;
    suite::round_trip(&ws).await;

    Response::ok("8 passed")
}

async fn wipe(ws: &Workspace) -> Result<()> {
    suite::wipe_root(ws)
        .await
        .map_err(|e| worker::Error::RustError(format!("wipe_root failed: {e}")))
}
