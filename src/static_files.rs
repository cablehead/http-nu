//! Serving files for `.static`, and the embedded Datastar bundles.
//!
//! Both go through `respond`: an ETag from the xxh3 hash of the bytes, the
//! conditional requests of RFC 9110 section 13, byte ranges (several as
//! multipart/byteranges), brotli for text, and the `?v=` cache rule.
//! A file is streamed from disk a chunk at a time.

use std::io::{Read, SeekFrom};
use std::ops::RangeInclusive;
use std::path::{Component, Path, PathBuf};

use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, StreamBody};
use hyper::body::Frame;
use hyper::header::{self, HeaderMap, HeaderName, HeaderValue};
use hyper::{Method, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::compression;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub type StaticResponse = hyper::Response<BoxBody<Bytes, BoxError>>;

pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// More ranges than this in one request, and the whole file is sent instead.
const MAX_RANGES: usize = 16;
const CHUNK: usize = 64 * 1024;

/// Where a response's bytes come from.
pub enum Source {
    File(PathBuf),
    /// Bytes built into the binary, with their brotli form.
    Embedded {
        plain: &'static [u8],
        brotli: &'static [u8],
    },
}

/// A file or embedded bytes, ready to send.
pub struct Asset {
    pub source: Source,
    pub len: u64,
    /// The xxh3 hash of the bytes, as 16 hex digits.
    pub hash: String,
    pub content_type: String,
    pub cache_control: &'static str,
}

/// Hash a file with xxh3, a chunk at a time, and return 16 hex digits.
fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(format!("{:016x}", hasher.digest()));
        }
        hasher.update(&buf[..n]);
    }
}

/// Hash bytes with xxh3 and return 16 hex digits.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(bytes))
}

/// Join a percent-encoded request path to root, or None when the path
/// leaves root.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    if decoded.contains('\0') {
        return None;
    }
    let mut file = root.to_path_buf();
    for part in Path::new(decoded.as_ref()).components() {
        match part {
            Component::Normal(name) => file.push(name),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(file)
}

/// The file `.static` serves for a request path under root: a directory
/// gives its index.html. None when there is no such file.
pub fn locate(root: &Path, path: &str) -> Option<PathBuf> {
    let mut file = resolve(root, path)?;
    if file.is_dir() {
        file.push("index.html");
    }
    file.is_file().then_some(file)
}

/// The version `.static-href` adds to a path as `?v=`: the first 8 hex
/// digits of the file's hash, or None when there is no file.
pub fn version(root: &Path, path: &str) -> Option<String> {
    let file = locate(root, path)?;
    hash_file(&file).ok().map(|h| h[..8].to_string())
}

/// The Content-Type for a media type: text gets `; charset=utf-8`.
pub fn content_type(essence: &str) -> String {
    if compressible(essence) && essence != "application/wasm" {
        format!("{essence}; charset=utf-8")
    } else {
        essence.to_string()
    }
}

fn compressible(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || ["javascript", "json", "xml", "svg", "wasm"]
            .iter()
            .any(|t| content_type.contains(t))
}

/// Answer a request for a path under root, as `.static` does.
pub async fn serve_path(
    req: &hyper::http::request::Parts,
    root: &Path,
    path: &str,
    fallback: Option<&str>,
) -> Result<StaticResponse, BoxError> {
    // A directory asked for without its slash: send the browser to the
    // slash, relative to where it is, so a mounted prefix is kept.
    if !path.is_empty() && !path.ends_with('/') && resolve(root, path).is_some_and(|p| p.is_dir()) {
        let last = path.rsplit('/').next().unwrap_or_default();
        let query = req.uri.query().map(|q| format!("?{q}")).unwrap_or_default();
        let mut res = bare(StatusCode::PERMANENT_REDIRECT);
        set(
            res.headers_mut(),
            header::LOCATION,
            &format!("{last}/{query}"),
        );
        return Ok(res);
    }
    let Some(file) = locate(root, path).or_else(|| fallback.and_then(|f| locate(root, f))) else {
        return Ok(bare(StatusCode::NOT_FOUND));
    };
    let len = tokio::fs::metadata(&file).await?.len();
    let hash = {
        let file = file.clone();
        tokio::task::spawn_blocking(move || hash_file(&file)).await??
    };
    let asked = req.uri.query().and_then(|q| {
        q.split('&')
            .find_map(|kv| kv.strip_prefix("v="))
            .map(str::to_string)
    });
    let cache_control = if asked.as_deref() == Some(&hash[..8]) {
        IMMUTABLE
    } else {
        "no-cache"
    };
    let content_type = content_type(
        mime_guess::from_path(&file)
            .first_or_octet_stream()
            .essence_str(),
    );
    let asset = Asset {
        source: Source::File(file),
        len,
        hash,
        content_type,
        cache_control,
    };
    respond(req, asset).await
}

/// Answer a GET or HEAD for an asset.
pub async fn respond(
    req: &hyper::http::request::Parts,
    asset: Asset,
) -> Result<StaticResponse, BoxError> {
    if req.method != Method::GET && req.method != Method::HEAD {
        let mut res = bare(StatusCode::METHOD_NOT_ALLOWED);
        set(res.headers_mut(), header::ALLOW, "GET, HEAD");
        return Ok(res);
    }

    // Brotli for text, unless a range is asked for: ranges are of the
    // plain bytes. Each form has its own ETag.
    let compressible = compressible(&asset.content_type);
    let brotli = compressible
        && compression::accepts_brotli(&req.headers)
        && !req.headers.contains_key(header::RANGE);
    let etag = if brotli {
        format!("\"{}-br\"", asset.hash)
    } else {
        format!("\"{}\"", asset.hash)
    };

    let mut headers = HeaderMap::new();
    set(&mut headers, header::ETAG, &etag);
    set(&mut headers, header::CACHE_CONTROL, asset.cache_control);
    set(&mut headers, header::ACCEPT_RANGES, "bytes");
    set(&mut headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    if compressible {
        set(&mut headers, header::VARY, "accept-encoding");
    }

    // Preconditions, in the order of RFC 9110 section 13.2.2.
    if let Some(v) = req.headers.get(header::IF_MATCH) {
        if !etag_matches(v, &etag, false) {
            return Ok(with(bare(StatusCode::PRECONDITION_FAILED), headers));
        }
    }
    if let Some(v) = req.headers.get(header::IF_NONE_MATCH) {
        if etag_matches(v, &etag, true) {
            return Ok(with(bare(StatusCode::NOT_MODIFIED), headers));
        }
    }
    set(&mut headers, header::CONTENT_TYPE, &asset.content_type);

    // A range, unless If-Range names some other version.
    let len = asset.len;
    let if_range_ok = req
        .headers
        .get(header::IF_RANGE)
        .is_none_or(|v| v.as_bytes() == etag.as_bytes());
    let ranges = match req.headers.get(header::RANGE) {
        Some(r) if if_range_ok => parse_ranges(r.to_str().unwrap_or(""), len),
        _ => Ranges::Whole,
    };

    let mut pieces = Vec::new();
    let status = match ranges {
        Ranges::Unsatisfiable => {
            set(
                &mut headers,
                header::CONTENT_RANGE,
                &format!("bytes */{len}"),
            );
            return Ok(with(bare(StatusCode::RANGE_NOT_SATISFIABLE), headers));
        }
        Ranges::Whole => {
            if brotli {
                set(&mut headers, header::CONTENT_ENCODING, "br");
            }
            match (&asset.source, brotli) {
                (Source::Embedded { brotli: br, .. }, true) => {
                    set(&mut headers, header::CONTENT_LENGTH, &br.len().to_string());
                    pieces.push(Piece::Static(br));
                }
                (Source::File(_), true) => pieces.push(Piece::Range(0..=len.max(1) - 1)),
                (_, false) => {
                    set(&mut headers, header::CONTENT_LENGTH, &len.to_string());
                    if len > 0 {
                        pieces.push(Piece::Range(0..=len - 1));
                    }
                }
            }
            StatusCode::OK
        }
        Ranges::Parts(parts) if parts.len() == 1 => {
            let (start, end) = (*parts[0].start(), *parts[0].end());
            set(
                &mut headers,
                header::CONTENT_RANGE,
                &format!("bytes {start}-{end}/{len}"),
            );
            set(
                &mut headers,
                header::CONTENT_LENGTH,
                &(end - start + 1).to_string(),
            );
            pieces.push(Piece::Range(start..=end));
            StatusCode::PARTIAL_CONTENT
        }
        Ranges::Parts(parts) => {
            // multipart/byteranges, RFC 9110 section 14.6
            let boundary = format!("http-nu-{}", asset.hash);
            let mut total = 0;
            for r in parts {
                let head = format!(
                    "\r\n--{boundary}\r\ncontent-type: {}\r\ncontent-range: bytes {}-{}/{len}\r\n\r\n",
                    asset.content_type,
                    r.start(),
                    r.end()
                );
                total += head.len() as u64 + r.end() - r.start() + 1;
                pieces.push(Piece::Text(head));
                pieces.push(Piece::Range(r));
            }
            let tail = format!("\r\n--{boundary}--\r\n");
            total += tail.len() as u64;
            pieces.push(Piece::Text(tail));
            set(
                &mut headers,
                header::CONTENT_TYPE,
                &format!("multipart/byteranges; boundary={boundary}"),
            );
            set(&mut headers, header::CONTENT_LENGTH, &total.to_string());
            StatusCode::PARTIAL_CONTENT
        }
    };

    let mut res = if req.method == Method::HEAD {
        bare(status)
    } else {
        // A file to compress goes through the brotli stream; embedded
        // bytes already have their brotli form.
        let compress = brotli && matches!(asset.source, Source::File(_));
        let rx = send(asset.source, pieces);
        let body = if compress {
            compression::compress_stream(rx)
        } else {
            BodyExt::boxed(StreamBody::new(
                ReceiverStream::new(rx).map(|chunk| Ok(Frame::data(Bytes::from(chunk)))),
            ))
        };
        let mut res = hyper::Response::new(body);
        *res.status_mut() = status;
        res
    };
    res.headers_mut().extend(headers);
    Ok(res)
}

/// Part of a response body, in order.
enum Piece {
    Text(String),
    Static(&'static [u8]),
    Range(RangeInclusive<u64>),
}

/// Send the pieces' bytes down a channel from a task of their own, a file
/// a chunk at a time. A read error ends the body early, so the client
/// sees it short of its length.
fn send(source: Source, pieces: Vec<Piece>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        let mut file = match &source {
            Source::File(path) => match tokio::fs::File::open(path).await {
                Ok(file) => Some(file),
                Err(_) => return,
            },
            Source::Embedded { .. } => None,
        };
        for piece in pieces {
            let sent = match (piece, &source, file.as_mut()) {
                (Piece::Text(text), _, _) => tx.send(text.into_bytes()).await.is_ok(),
                (Piece::Static(bytes), _, _) => tx.send(bytes.to_vec()).await.is_ok(),
                (Piece::Range(r), Source::Embedded { plain, .. }, _) => {
                    let slice = &plain[*r.start() as usize..=*r.end() as usize];
                    tx.send(slice.to_vec()).await.is_ok()
                }
                (Piece::Range(r), Source::File(_), Some(file)) => copy_range(file, r, &tx).await,
                _ => false,
            };
            if !sent {
                return;
            }
        }
    });
    rx
}

/// Send a range of a file down the channel, a chunk at a time.
async fn copy_range(
    file: &mut tokio::fs::File,
    range: RangeInclusive<u64>,
    tx: &mpsc::Sender<Vec<u8>>,
) -> bool {
    if file.seek(SeekFrom::Start(*range.start())).await.is_err() {
        return false;
    }
    let mut left = range.end() - range.start() + 1;
    let mut buf = vec![0; CHUNK];
    while left > 0 {
        let want = left.min(CHUNK as u64) as usize;
        match file.read(&mut buf[..want]).await {
            Ok(0) | Err(_) => return false,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).await.is_err() {
                    return false;
                }
                left -= n as u64;
            }
        }
    }
    true
}

enum Ranges {
    Whole,
    Parts(Vec<RangeInclusive<u64>>),
    Unsatisfiable,
}

/// Parse a Range header against a length: `bytes=0-99`, `bytes=100-`,
/// `bytes=-100`, or several, comma separated. Overlapping and adjacent
/// ranges are merged. Anything this can't read, or more than MAX_RANGES,
/// means the whole file, as RFC 9110 allows.
fn parse_ranges(value: &str, len: u64) -> Ranges {
    let Some(specs) = value.trim().strip_prefix("bytes=") else {
        return Ranges::Whole;
    };
    let last = len.saturating_sub(1);
    let mut parts = Vec::new();
    for spec in specs.split(',') {
        let Some((a, b)) = spec.trim().split_once('-') else {
            return Ranges::Whole;
        };
        let (a, b) = (a.trim(), b.trim());
        let range = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(start), Ok(end)) if start <= end => start..=end.min(last),
            (Ok(start), Err(_)) if b.is_empty() => start..=last,
            (Err(_), Ok(n)) if a.is_empty() && n > 0 => len.saturating_sub(n)..=last,
            _ => return Ranges::Whole,
        };
        if *range.start() < len {
            parts.push(range);
        }
    }
    if parts.is_empty() {
        return Ranges::Unsatisfiable;
    }
    parts.sort_by_key(|r| *r.start());
    let mut merged: Vec<RangeInclusive<u64>> = Vec::new();
    for r in parts {
        match merged.last_mut() {
            Some(prev) if *r.start() <= prev.end() + 1 => {
                *prev = *prev.start()..=(*prev.end()).max(*r.end());
            }
            _ => merged.push(r),
        }
    }
    if merged.len() > MAX_RANGES {
        return Ranges::Whole;
    }
    Ranges::Parts(merged)
}

/// Whether an If-Match or If-None-Match value names this ETag. A weak
/// comparison ignores a `W/` prefix; a strong one never matches it.
fn etag_matches(value: &HeaderValue, etag: &str, weak: bool) -> bool {
    let Ok(value) = value.to_str() else {
        return false;
    };
    value.trim() == "*"
        || value
            .split(',')
            .map(str::trim)
            .any(|tag| match tag.strip_prefix("W/") {
                Some(tag) => weak && tag == etag,
                None => tag == etag,
            })
}

fn bare(status: StatusCode) -> StaticResponse {
    let mut res = hyper::Response::new(Empty::new().map_err(|never| match never {}).boxed());
    *res.status_mut() = status;
    res
}

fn with(mut res: StaticResponse, headers: HeaderMap) -> StaticResponse {
    res.headers_mut().extend(headers);
    res
}

fn set(headers: &mut HeaderMap, name: HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}
