//! Path helpers for the Workspace. Pure-Rust, sync, testable on
//! desktop -- the unit tests run via `cargo test` even though the rest
//! of the workspace module is wasm32+cloudflare gated.

/// Collapse `.`, `..`, and double slashes. Force a leading `/`. Result
/// is always absolute, never has a trailing slash (except for the root
/// itself).
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
}
