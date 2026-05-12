//! Generic conformance suite for the `FileSystem` trait.
//!
//! # Why this exists
//!
//! `InMemoryFs` is a *behavioural double* for `Workspace`. The danger
//! of behavioural doubles is they drift: the in-memory impl gets
//! "convenient" semantics that diverge from the real impl, tests pass
//! against the double, then production breaks against the real backend.
//! That's a worse failure mode than no tests at all -- it ships false
//! confidence.
//!
//! The defence is **discipline**: tests that exercise FS behaviour
//! belong here, written against `<F: FileSystem>`. Each function is
//! then run twice:
//!
//! 1. **Against `InMemoryFs`** in the unit-test module of
//!    `in_memory_fs.rs`. Runs under `cargo test` on desktop.
//! 2. **Against `Workspace`** via `mise run cf:dev` + an integration
//!    harness inside a DurableObject. (Today this step is manual
//!    curl-based; the harness can be automated later.)
//!
//! If a conformance assertion passes against the double but fails
//! against the real backend, the assertion is wrong -- not the
//! backend. Fix the conformance test to express the *real* invariant,
//! then make both impls satisfy it.
//!
//! # What belongs here
//!
//! Properties that are part of the `FileSystem` contract -- the things
//! a caller is allowed to assume regardless of backend. Examples:
//!
//! - "After `write_file(p, x)`, `read_file(p)` returns `Some(x)`."
//! - "`stat` on a missing path returns `Ok(None)`, not `Err(_)`."
//! - "`read_file_bytes` on a directory returns `Err(IsDir(_))`."
//! - "`on_change` fires Create on first write, Update on subsequent."
//!
//! # What does NOT belong here
//!
//! Properties that are backend-specific. Anti-examples:
//!
//! - "Files larger than 1.5MB spill to R2" -- Workspace-only.
//! - "Tests survive DurableObject eviction" -- Workspace-only.
//! - "Lookups are O(1)" -- InMemoryFs-only.
//!
//! Backend-specific tests live next to their impl, not here.

use crate::shell::{
    error::FsError, CpOptions, EntryType, FileSystem, MkdirOptions, OnChange, RmOptions,
    WorkspaceChangeType, MAX_PATH_LENGTH,
};
use std::sync::{Arc, Mutex};

pub async fn round_trip<F: FileSystem>(fs: &F) {
    fs.write_file("/hello.txt", "world", None).await.unwrap();
    let read = fs.read_file("/hello.txt").await.unwrap();
    assert_eq!(read.as_deref(), Some("world"));

    fs.write_file_bytes("/blob.bin", &[1, 2, 3, 4], None)
        .await
        .unwrap();
    let bytes = fs.read_file_bytes("/blob.bin").await.unwrap();
    assert_eq!(bytes.as_deref(), Some(&[1, 2, 3, 4][..]));
}

pub async fn enoent_returns_ok_none<F: FileSystem>(fs: &F) {
    // Deviation from upstream `FileSystem` interface: we return
    // `Ok(None)` rather than throwing ENOENT. This is part of the
    // contract every impl must follow. (EISDIR / ENOTDIR / etc. are
    // still `Err`.)
    assert!(matches!(fs.stat("/missing").await, Ok(None)));
    assert!(matches!(fs.lstat("/missing").await, Ok(None)));
    assert!(matches!(fs.read_file("/missing").await, Ok(None)));
    assert!(matches!(fs.read_file_bytes("/missing").await, Ok(None)));
    assert!(matches!(fs.readlink("/missing").await, Ok(None)));
    assert!(matches!(fs.realpath("/missing").await, Ok(None)));
}

pub async fn eisdir_on_read_of_directory<F: FileSystem>(fs: &F) {
    fs.mkdir("/d", MkdirOptions::default()).await.unwrap();
    match fs.read_file_bytes("/d").await {
        Err(FsError::IsDir(_)) => {}
        other => panic!("expected EISDIR, got {other:?}"),
    }
}

pub async fn eisdir_on_write_to_root<F: FileSystem>(fs: &F) {
    match fs.write_file("/", "x", None).await {
        Err(FsError::IsDir(_)) => {}
        other => panic!("expected EISDIR on root write, got {other:?}"),
    }
}

pub async fn name_too_long<F: FileSystem>(fs: &F) {
    let long = "/".to_string() + &"a".repeat(MAX_PATH_LENGTH);
    match fs.write_file(&long, "x", None).await {
        Err(FsError::NameTooLong(_)) => {}
        other => panic!("expected ENAMETOOLONG, got {other:?}"),
    }
}

pub async fn rm_recursive<F: FileSystem>(fs: &F) {
    fs.mkdir("/d/a/b", MkdirOptions { recursive: true })
        .await
        .unwrap();
    fs.write_file("/d/a/leaf.txt", "x", None).await.unwrap();
    fs.write_file("/d/top.txt", "y", None).await.unwrap();

    // Non-recursive rm of a non-empty dir must error.
    match fs.rm("/d", RmOptions::default()).await {
        Err(FsError::NotEmpty(_)) => {}
        other => panic!("expected ENOTEMPTY, got {other:?}"),
    }

    fs.rm(
        "/d",
        RmOptions {
            recursive: true,
            force: false,
        },
    )
    .await
    .unwrap();
    assert!(matches!(fs.stat("/d").await, Ok(None)));
    assert!(matches!(fs.stat("/d/a/b").await, Ok(None)));
    assert!(matches!(fs.stat("/d/a/leaf.txt").await, Ok(None)));
}

pub async fn cp_preserves_mime<F: FileSystem>(fs: &F) {
    fs.write_file_bytes("/src.png", &[137, 80, 78, 71], Some("image/png"))
        .await
        .unwrap();
    fs.cp("/src.png", "/dst.png", CpOptions::default())
        .await
        .unwrap();
    let dst_stat = fs.stat("/dst.png").await.unwrap().unwrap();
    assert_eq!(dst_stat.mime_type, "image/png");
    assert_eq!(dst_stat.kind, EntryType::File);
}

pub async fn on_change_emits_create_then_update_then_delete<F: FileSystem + 'static>(fs: &F) {
    let events: Arc<Mutex<Vec<(WorkspaceChangeType, String, EntryType)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let events_for_cb = events.clone();
    let cb: OnChange = Arc::new(move |e| {
        events_for_cb
            .lock()
            .unwrap()
            .push((e.kind, e.path, e.entry_type));
    });
    set_on_change(fs, cb);

    fs.write_file("/x.txt", "a", None).await.unwrap();
    fs.write_file("/x.txt", "b", None).await.unwrap();
    fs.rm("/x.txt", RmOptions::default()).await.unwrap();

    let got = events.lock().unwrap().clone();
    assert_eq!(
        got,
        vec![
            (WorkspaceChangeType::Create, "/x.txt".to_string(), EntryType::File),
            (WorkspaceChangeType::Update, "/x.txt".to_string(), EntryType::File),
            (WorkspaceChangeType::Delete, "/x.txt".to_string(), EntryType::File),
        ]
    );
}

/// Wipe every entry under `/`. Each conformance fn assumes a fresh
/// filesystem; harnesses call this between fns when reusing one
/// backend instance. Desktop tests construct a fresh `InMemoryFs` per
/// fn so they don't need it; the wasm harness in
/// `crate::cf::conformance` does, because `Workspace` (DO SQLite + R2)
/// persists.
pub async fn wipe_root<F: FileSystem>(fs: &F) -> crate::shell::Result<()> {
    let entries = fs.read_dir_with_file_types("/").await?.unwrap_or_default();
    for e in entries {
        let path = format!("/{}", e.name);
        fs.rm(
            &path,
            RmOptions {
                recursive: true,
                force: true,
            },
        )
        .await?;
    }
    Ok(())
}

/// Per-backend `set_on_change` wrapper. The trait can't carry it --
/// not every future `FileSystem` impl should be forced to support
/// listeners, and the trait's `&self` receivers don't combine with a
/// `&mut self` setter. The downcast keeps the trait minimal while
/// letting the conformance suite verify listener behavior.
///
/// When adding a new `FileSystem` backend that supports listeners,
/// add a downcast arm here.
fn set_on_change<F: FileSystem + 'static>(fs: &F, cb: OnChange) {
    let any = fs as &dyn std::any::Any;
    if let Some(fs) = any.downcast_ref::<crate::shell::InMemoryFs>() {
        fs.set_on_change(cb);
        return;
    }
    #[cfg(all(feature = "cloudflare", target_arch = "wasm32"))]
    if let Some(fs) = any.downcast_ref::<crate::cf::shell::Workspace>() {
        fs.set_on_change(cb);
        return;
    }
    panic!(
        "conformance::set_on_change: backend doesn't have a known setter. \
         Add a downcast arm here for your FileSystem impl."
    );
}
