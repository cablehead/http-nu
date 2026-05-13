//! Shared helpers for the Vfs-backed shadow commands.

use nu_engine::command_prelude::*;

use crate::vfs::{with_vfs, Vfs};

/// "."/""/"./" -> "/"; bare names get a leading slash. Workspace paths
/// are absolute by convention.
pub fn normalise_input(p: &str) -> String {
    if p.is_empty() || p == "." || p == "./" {
        return "/".to_string();
    }
    if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

pub fn vfs_err(
    span: nu_protocol::Span,
    msg: impl Into<String>,
    error: impl Into<String>,
) -> ShellError {
    ShellError::GenericError {
        msg: msg.into(),
        error: error.into(),
        span: Some(span),
        help: None,
        inner: Vec::new(),
    }
}

pub fn no_vfs(span: nu_protocol::Span) -> ShellError {
    vfs_err(span, "Workspace not loaded", "no vfs installed")
}

/// Run `f` against the active Vfs, returning ShellError if no Vfs is
/// installed (e.g. desktop running this binary in non-CF mode, or a CF
/// request that bypassed `install_vfs`).
pub fn require_vfs<R>(
    span: nu_protocol::Span,
    f: impl FnOnce(&dyn Vfs) -> Result<R, ShellError>,
) -> Result<R, ShellError> {
    with_vfs(|v| match v {
        Some(v) => f(v),
        None => Err(no_vfs(span)),
    })
}
