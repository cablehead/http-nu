//! Conformance **runner**: drives `cloudflare_shell::conformance`'s
//! generic `<F: FileSystem>` test suite against a real `Workspace`
//! (DO SQLite + R2) and returns the result as a `worker::Response`.
//!
//! Two modules, same word, different jobs -- the source of perennial
//! confusion when reading this crate:
//!
//! | Module | What it is |
//! |---|---|
//! | [`cloudflare_shell::conformance`] | the **suite** -- generic `<F: FileSystem>` test functions. Pure, backend-agnostic. |
//! | `cloudflare_shell_workspace::conformance_runner` (this file) | the **runner** -- constructs a real `Workspace`, calls each suite function against it, wraps it in HTTP response shape. wasm-only. |
//!
//! Wire this up from any `worker::Route` (or any handler that has the
//! caller's `SqlStorage` + `Bucket` in hand) to prove the DO SQLite +
//! R2 backend matches the trait contract. Useful for CI smoke tests
//! and for catching schema-compat regressions before they hit users.
//!
//! Today, two callers use it: http-nu's `src/cf/mod.rs` (serves it at
//! `GET /<user>/_workspace/conformance`) and any future Worker that
//! embeds `Workspace`.
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
//! State: each fn assumes a fresh filesystem, so the runner calls
//! `wipe_root` between fns. It uses namespace `conformance` (valid
//! per `VALID_NAMESPACE`) to keep its state segregated from real data.

use futures_util::stream::{self, StreamExt};
use worker::{Bucket, Response, Result, SqlStorage};

use cloudflare_shell::{conformance as suite, error::FsError, interface::MkdirOptions};

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

    // Workspace-only: streams aren't on the FileSystem trait, so these
    // tests live next to the impl (not in cloudflare-shell::conformance).
    wipe(&ws).await?;
    stream_round_trip(&ws).await;

    wipe(&ws).await?;
    write_stream_round_trip(&ws).await;

    wipe(&ws).await?;
    stream_read_enoent(&ws).await;

    wipe(&ws).await?;
    stream_read_eisdir(&ws).await;

    wipe(&ws).await?;
    streaming_writes_toggle_state(&ws).await;

    wipe(&ws).await?;
    file_exists_distinguishes_file_dir_missing(&ws).await;

    wipe(&ws).await?;
    delete_file_removes_files_refuses_dirs(&ws).await;

    wipe(&ws).await?;
    get_workspace_info_aggregates(&ws).await;

    Response::ok("16 passed")
}

/// `read_file_stream` round-trip via `write_file_bytes`. Drains the
/// returned stream and re-assembles the original bytes.
async fn stream_round_trip(ws: &Workspace) {
    let payload: Vec<u8> = (0..256u32).map(|i| (i % 251) as u8).collect();
    ws.write_file_bytes("/stream-rt.bin", &payload, Some("application/octet-stream"))
        .await
        .unwrap();
    let mut s = ws
        .read_file_stream("/stream-rt.bin")
        .await
        .unwrap()
        .expect("read_file_stream missing for known path");
    let mut got: Vec<u8> = Vec::new();
    while let Some(chunk) = s.next().await {
        got.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(got, payload);
}

/// `write_file_stream` round-trip via `read_file_bytes`. Feeds a
/// multi-chunk stream and verifies the file holds the concatenated bytes.
async fn write_stream_round_trip(ws: &Workspace) {
    let chunks: Vec<std::result::Result<Vec<u8>, FsError>> = vec![
        Ok(b"chunk-1 ".to_vec()),
        Ok(b"chunk-2 ".to_vec()),
        Ok(b"chunk-3".to_vec()),
    ];
    let s = stream::iter(chunks);
    ws.write_file_stream("/stream-wr.txt", s, Some("text/plain"))
        .await
        .unwrap();
    let got = ws
        .read_file_bytes("/stream-wr.txt")
        .await
        .unwrap()
        .expect("write_file_stream did not persist file");
    assert_eq!(got, b"chunk-1 chunk-2 chunk-3");
}

/// `read_file_stream` on a missing path returns `Ok(None)` (matches the
/// crate-wide ENOENT deviation).
async fn stream_read_enoent(ws: &Workspace) {
    let r = ws.read_file_stream("/no-such-stream").await.unwrap();
    assert!(r.is_none(), "expected Ok(None) on ENOENT");
}

/// `read_file_stream` on a directory returns `Err(IsDir)` (matches
/// `read_file_bytes` on a dir).
async fn stream_read_eisdir(ws: &Workspace) {
    ws.mkdir("/stream-dir", MkdirOptions::default())
        .await
        .unwrap();
    // `dyn Stream` isn't `Debug`, so summarise the variant by hand
    // instead of `{:?}`-printing the whole `Result`.
    match ws.read_file_stream("/stream-dir").await {
        Err(FsError::IsDir(_)) => {}
        Err(e) => panic!("expected EISDIR on read_file_stream of dir, got error: {e}"),
        Ok(None) => panic!("expected EISDIR on read_file_stream of dir, got Ok(None)"),
        Ok(Some(_)) => panic!("expected EISDIR on read_file_stream of dir, got Ok(Some(<stream>))"),
    }
}

/// Toggle round-trip. Verifies the public getter mirrors the setter --
/// the EFBIG cap behavior itself isn't exercised here (would require
/// allocating > `MAX_STREAM_SIZE` = 100 MB inside the wasm conformance
/// harness). Behavior is documented + asserted by code review.
async fn streaming_writes_toggle_state(ws: &Workspace) {
    assert!(!ws.streaming_writes(), "default must be OFF");
    ws.set_streaming_writes(true);
    assert!(ws.streaming_writes(), "setter ON must be observable");
    ws.set_streaming_writes(false);
    assert!(!ws.streaming_writes(), "setter OFF must be observable");
}

/// `file_exists` is true only for files (after symlink resolution), not
/// directories or missing paths.
async fn file_exists_distinguishes_file_dir_missing(ws: &Workspace) {
    ws.write_file("/exists.txt", "x", None).await.unwrap();
    ws.mkdir("/somedir", MkdirOptions::default()).await.unwrap();
    ws.symlink("/exists.txt", "/link-to-file").await.unwrap();

    assert!(ws.file_exists("/exists.txt").await.unwrap(), "file");
    assert!(
        !ws.file_exists("/somedir").await.unwrap(),
        "dir is not a file"
    );
    assert!(!ws.file_exists("/missing").await.unwrap(), "missing");
    assert!(
        ws.file_exists("/link-to-file").await.unwrap(),
        "symlink resolves to file"
    );
}

/// `delete_file` removes files (returning true), is a no-op for missing
/// (returning false), and refuses directories (Err IsDir).
async fn delete_file_removes_files_refuses_dirs(ws: &Workspace) {
    ws.write_file("/d.txt", "x", None).await.unwrap();
    assert!(ws.delete_file("/d.txt").await.unwrap(), "removes existing");
    assert!(
        matches!(ws.stat("/d.txt").await, Ok(None)),
        "gone after delete"
    );

    assert!(
        !ws.delete_file("/never-existed").await.unwrap(),
        "false on missing"
    );

    ws.mkdir("/adir", MkdirOptions::default()).await.unwrap();
    match ws.delete_file("/adir").await {
        Err(FsError::IsDir(_)) => {}
        other => panic!("expected EISDIR on delete_file of dir, got {other:?}"),
    }
}

/// `get_workspace_info` totals match an explicit seed: two files (one
/// inline, one would be R2 if bound, but conformance covers the SQL
/// row counts -- not the storage_backend split, which is a Workspace
/// invariant exercised elsewhere) and one directory.
async fn get_workspace_info_aggregates(ws: &Workspace) {
    ws.write_file("/a.txt", "hello", None).await.unwrap();
    ws.write_file_bytes("/b.bin", &[0u8; 7], None)
        .await
        .unwrap();
    ws.mkdir("/d1", MkdirOptions::default()).await.unwrap();

    let info = ws.get_workspace_info().await.unwrap();
    assert_eq!(info.file_count, 2, "2 files");
    // `/` (root) + `/d1` = 2 directories.
    assert_eq!(info.directory_count, 2, "2 directories (root + /d1)");
    assert_eq!(info.total_bytes, 5 + 7, "total bytes = 12");
    // r2_file_count is exercised only with R2 bound; in default
    // conformance runs (no R2), it should be 0.
    if ws.has_r2() {
        // can't assert exact count without knowing whether files spilled
    } else {
        assert_eq!(info.r2_file_count, 0, "no R2 = no R2 files");
    }
}

async fn wipe(ws: &Workspace) -> Result<()> {
    suite::wipe_root(ws)
        .await
        .map_err(|e| worker::Error::RustError(format!("wipe_root failed: {e}")))
}
