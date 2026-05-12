//! Upstream: `.src/agents/packages/shell/src/fs/path-utils.ts`.
//!
//! Pure path utilities. No worker-rs deps, no FS deps, no time deps --
//! safe to use from desktop tests. `normalize_path` is the
//! length-validating wrapper that every public FS method calls at its
//! entry point (matches upstream's inline `MAX_PATH_LENGTH` check at
//! filesystem.ts:1584).

use crate::shell::{FsError, Result, MAX_PATH_LENGTH};

/// Upstream: `fs/path-utils.ts:13` `normalizePath()`. Collapse `.`,
/// `..`, and double slashes; force a leading `/`. Result is always
/// absolute, never has a trailing slash (except for the root itself).
pub fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        "/".to_string()
    } else {
        let mut s = String::with_capacity(path.len() + 1);
        for seg in out {
            s.push('/');
            s.push_str(seg);
        }
        s
    }
}

/// Upstream: `filesystem.ts:1584` raises `ENAMETOOLONG: path exceeds
/// ${MAX_PATH_LENGTH} characters` inside `normalizePath`. We mirror by
/// wrapping `normalize` in a `Result`-returning validator that every
/// public FS method calls.
pub fn normalize_path(path: &str) -> Result<String> {
    let p = normalize(path);
    if p.len() > MAX_PATH_LENGTH {
        return Err(FsError::NameTooLong(format!(
            "path exceeds {MAX_PATH_LENGTH} characters"
        )));
    }
    Ok(p)
}

/// Parent directory of `path`. The root's parent is the empty string
/// (matches @cloudflare/shell's invariant: root has parent_path = '').
pub fn parent_path(path: &str) -> String {
    if path == "/" {
        return String::new();
    }
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// Leaf name. Root's name is the empty string (matches @cloudflare/shell).
pub fn path_name(path: &str) -> String {
    if path == "/" {
        return String::new();
    }
    match path.rfind('/') {
        Some(i) => path[i + 1..].to_string(),
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basics() {
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize(""), "/");
        assert_eq!(normalize("/foo"), "/foo");
        assert_eq!(normalize("foo"), "/foo");
        assert_eq!(normalize("/foo/"), "/foo");
        assert_eq!(normalize("/foo/bar"), "/foo/bar");
        assert_eq!(normalize("/foo//bar"), "/foo/bar");
        assert_eq!(normalize("/foo/./bar"), "/foo/bar");
        assert_eq!(normalize("/foo/../bar"), "/bar");
        assert_eq!(normalize("/foo/bar/.."), "/foo");
        assert_eq!(normalize("/foo/bar/../baz"), "/foo/baz");
    }

    #[test]
    fn parent_path_basics() {
        assert_eq!(parent_path("/"), "");
        assert_eq!(parent_path("/foo"), "/");
        assert_eq!(parent_path("/foo/bar"), "/foo");
        assert_eq!(parent_path("/a/b/c"), "/a/b");
    }

    #[test]
    fn path_name_basics() {
        assert_eq!(path_name("/"), "");
        assert_eq!(path_name("/foo"), "foo");
        assert_eq!(path_name("/foo/bar"), "bar");
        assert_eq!(path_name("/a/b/c"), "c");
    }

    #[test]
    fn normalize_path_rejects_overlong() {
        let long = "/".to_string() + &"a".repeat(MAX_PATH_LENGTH);
        assert!(normalize_path(&long).is_err());
    }
}
