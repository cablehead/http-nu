//! Wire helpers for the entrypoint <-> DO boundary.
//!
//! The two sides talk over an internal `stub.fetch_with_request(...)`
//! using JSON-shaped requests. This module keeps the encode/decode
//! ceremony in one place.
//!
//! - Method: `POST`
//! - URL: `https://shell-fs-rpc/<method>` (the host doesn't matter; the
//!   DO routes on path).
//! - Request body: JSON of the matching `*Req` struct.
//! - Response body on Ok: JSON of the matching `*Resp` struct (or `{}` for `Ack`).
//! - Response body on Err: JSON of `RpcError`; status code = 500.

use cloudflare_shell::FsError;
use cloudflare_shell_rpc_types::RpcError;
use serde::{de::DeserializeOwned, Serialize};
use worker::{Method, Request, RequestInit, Response, Result, Stub};

/// Build the internal fetch request that the entrypoint sends into the
/// DO. `path` is e.g. `/read_file`.
pub fn build_request<T: Serialize>(path: &str, body: &T) -> Result<Request> {
    let json = serde_json::to_string(body)
        .map_err(|e| worker::Error::RustError(format!("encode {path}: {e}")))?;
    let url = format!("https://shell-fs-rpc{path}");
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(json.into()));
    Request::new_with_init(&url, &init)
}

/// On the entrypoint side: send the request, decode the response, or
/// surface `RpcError` if the DO returned an error body.
pub async fn call_do<T: DeserializeOwned>(stub: &Stub, req: Request) -> Result<T> {
    let mut resp = stub.fetch_with_request(req).await?;
    let status = resp.status_code();
    let bytes = resp.bytes().await?;
    if (200..300).contains(&status) {
        serde_json::from_slice::<T>(&bytes)
            .map_err(|e| worker::Error::RustError(format!("decode response: {e}")))
    } else {
        let err: RpcError = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| RpcError::Other(String::from_utf8_lossy(&bytes).to_string()));
        Err(worker::Error::RustError(rpc_error_to_string(&err)))
    }
}

/// Inverse of `From<FsError>`: render an `FsError` as the wire-side
/// `RpcError`. Keeps the wire stable against in-process error refactors.
pub fn fs_error_to_rpc(e: FsError) -> RpcError {
    match e {
        FsError::NotFound(m) => RpcError::NotFound(m),
        FsError::IsDir(m) => RpcError::IsDir(m),
        FsError::NotDir(m) => RpcError::NotDir(m),
        FsError::NotEmpty(m) => RpcError::NotEmpty(m),
        FsError::NameTooLong(m) => RpcError::NameTooLong(m),
        FsError::SymlinkLoop(m) => RpcError::SymlinkLoop(m),
        FsError::InvalidEncoding(m) => RpcError::InvalidUtf8(m),
        FsError::Io(m) => RpcError::Io(m),
        FsError::NoSpace(m) => RpcError::NoSpace(m),
        FsError::Other(m) => RpcError::Other(m),
    }
}

fn rpc_error_to_string(err: &RpcError) -> String {
    match err {
        RpcError::NotFound(m) => format!("ENOENT: {m}"),
        RpcError::IsDir(m) => format!("EISDIR: {m}"),
        RpcError::NotDir(m) => format!("ENOTDIR: {m}"),
        RpcError::AlreadyExists(m) => format!("EEXIST: {m}"),
        RpcError::NotEmpty(m) => format!("ENOTEMPTY: {m}"),
        RpcError::NameTooLong(m) => format!("ENAMETOOLONG: {m}"),
        RpcError::SymlinkLoop(m) => format!("ELOOP: {m}"),
        RpcError::InvalidUtf8(m) => format!("EILSEQ: {m}"),
        RpcError::NoSpace(m) => format!("ENOSPC: {m}"),
        RpcError::Io(m) => format!("EIO: {m}"),
        RpcError::Other(m) => m.clone(),
    }
}

/// On the DO side: build a 500 response body carrying `RpcError`.
pub fn err_response(err: RpcError) -> Result<Response> {
    let body = serde_json::to_string(&err)
        .unwrap_or_else(|_| r#"{"code":"Other","message":"serialize failed"}"#.to_string());
    Response::error(body, 500)
}

/// On the DO side: build a 200 response body carrying a typed result.
pub fn ok_response<T: Serialize>(value: &T) -> Result<Response> {
    let body = serde_json::to_string(value)
        .map_err(|e| worker::Error::RustError(format!("encode response: {e}")))?;
    Response::ok(body)
}
