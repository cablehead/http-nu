//! Wasm-side runner for `crate::shell::conformance`'s generic `FileSystem`
//! tests. Drives them against a real `Workspace` inside a Durable
//! Object so the same assertions that pass against `InMemoryFs` on
//! desktop also pass against the DO SQLite + R2 backend in
//! production.
//!
//! Why this exists: see `src/shell/CLAUDE.md` -- the mock-divergence
//! discipline. Conformance tests against `InMemoryFs` only catch
//! divergence if those same tests also run against `Workspace`. This
//! module is the second leg of that loop.
//!
//! How to invoke:
//!
//!   mise run cf:dev      # in one terminal
//!   curl -i http://127.0.0.1:8787/alice/_workspace/conformance
//!
//! Output: `200 OK` + plain text `<n> passed` if every assertion
//! holds. On any assertion failure the worker fetch handler returns
//! `500` with the panic message; `mise run cf:tail` (or the dev
//! terminal) prints the backtrace.
//!
//! Each conformance fn assumes a fresh filesystem, so `wipe_root` runs
//! between fns. The harness uses namespace `__conformance` to keep its
//! state segregated from the user's real workspace.

use worker::{Bucket, Response, Result, SqlStorage};

use super::shell::Workspace;
use crate::shell::conformance as suite;

const CONFORMANCE_NAMESPACE: &str = "__conformance";

pub(super) async fn run(sql: SqlStorage, r2: Option<Bucket>) -> Result<Response> {
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
    suite::on_change_emits_create_then_update_then_delete(&ws).await;

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
