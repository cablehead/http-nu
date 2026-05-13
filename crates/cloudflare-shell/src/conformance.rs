//! Generic conformance suite for the `FileSystem` trait.
//!
//! # Why this exists
//!
//! Test code exercising FS behaviour belongs here, written against
//! `<F: FileSystem>`, so the same assertions can run against any impl.
//! Today the only impl is `Workspace`, which is wasm-only; the wasm
//! harness at `src/cf/conformance.rs` invokes these functions against
//! a real DurableObject + R2 backend, exposed as
//! `GET /<user>/_workspace/conformance`.
//!
//! If a second `FileSystem` impl is added later, every function here
//! works against it unchanged.
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
//!
//! Backend-specific tests live next to their impl, not here.

use crate::{
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

/// Conformance fn for change-listener wiring. `set_on_change` isn't on
/// the `FileSystem` trait (impls that don't support listeners
/// shouldn't be forced to stub it), so the harness passes the setter
/// in as a closure. Workspace's call site:
/// `|ws, cb| ws.set_on_change(cb)`.
pub async fn on_change_emits_create_then_update_then_delete<F: FileSystem>(
    fs: &F,
    set_on_change: impl FnOnce(&F, OnChange),
) {
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
            (
                WorkspaceChangeType::Create,
                "/x.txt".to_string(),
                EntryType::File
            ),
            (
                WorkspaceChangeType::Update,
                "/x.txt".to_string(),
                EntryType::File
            ),
            (
                WorkspaceChangeType::Delete,
                "/x.txt".to_string(),
                EntryType::File
            ),
        ]
    );
}

/// Wipe every entry under `/`. The wasm harness in
/// `crate::cf::conformance` calls this between fns because `Workspace`
/// (DO SQLite + R2) persists -- each fn assumes a fresh filesystem.
pub async fn wipe_root<F: FileSystem>(fs: &F) -> crate::Result<()> {
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
