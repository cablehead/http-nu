//! Build-time guard: every `#[wasm_bindgen(js_name = X)]` in src/rpc.rs
//! must have a matching `async X(args)` method in shim.js.
//!
//! Why: worker-rs 0.8's worker-build doesn't auto-wire env-injection
//! for non-fetch RPC exports, so we own `shim.js` by hand (see the
//! file's own doc-comment for the env-injection rationale). The cost
//! of hand-rolling is that the Rust side and the JS side can drift
//! silently -- a new `#[wasm_bindgen(js_name = foo)]` compiles green
//! but the WorkerEntrypoint never exposes `foo()`, so service-binding
//! callers see `TypeError: env.SHELL_FS.foo is not a function` at
//! runtime instead of at build time.
//!
//! This build script closes that gap by parsing both files with
//! simple regex-grade heuristics and emitting a `cargo:error=` if any
//! exported RPC method is missing from `shim.js`. Cheap insurance.
//!
//! Drops automatically the day worker-rs grows native env-injection
//! and we delete the custom shim.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rpc_rs = manifest_dir.join("src").join("rpc.rs");
    let shim_js = manifest_dir.join("shim.js");

    // Rerun whenever either file changes.
    println!("cargo:rerun-if-changed={}", rpc_rs.display());
    println!("cargo:rerun-if-changed={}", shim_js.display());

    let rpc_src = match fs::read_to_string(&rpc_rs) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "cargo:warning=shim-drift-guard: cannot read {}: {e}",
                rpc_rs.display()
            );
            return;
        }
    };
    let shim_src = match fs::read_to_string(&shim_js) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "cargo:warning=shim-drift-guard: cannot read {}: {e}",
                shim_js.display()
            );
            return;
        }
    };

    let rust_methods = extract_js_names(&rpc_src);
    let shim_methods = extract_shim_methods(&shim_src);

    let missing_in_shim: Vec<_> = rust_methods.difference(&shim_methods).collect();
    let missing_in_rust: Vec<_> = shim_methods.difference(&rust_methods).collect();

    for name in &missing_in_rust {
        // Stale shim method = JS dead code. Warn but don't fail.
        println!(
            "cargo:warning=shim-drift: shim.js defines method `{name}` but src/rpc.rs has no matching #[wasm_bindgen(js_name = {name})] export. Stale shim method?"
        );
    }

    if !missing_in_shim.is_empty() {
        // A missing-in-shim entry is a runtime TypeError waiting to
        // happen. `cargo:error=` is informational in stable cargo;
        // the only portable way to actually fail the build from a
        // build script is to exit non-zero, with the diagnostic on
        // stderr so it surfaces in the user's terminal.
        for name in &missing_in_shim {
            eprintln!(
                "error: shim-drift: #[wasm_bindgen(js_name = {name})] is exported by src/rpc.rs but shim.js has no matching `async {name}(...)` method. Add it to the Entrypoint class in shim.js -- service-binding callers won't see this method otherwise."
            );
        }
        std::process::exit(1);
    }
}

/// Pull every `js_name = <ident>` from `#[wasm_bindgen(...)]` attrs.
/// Accepts either bare or quoted form: `js_name = foo` / `js_name = "foo"`.
fn extract_js_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let l = line.trim();
        if !l.starts_with("#[wasm_bindgen") {
            continue;
        }
        let Some(rest) = l.split("js_name").nth(1) else {
            continue;
        };
        let rest = rest.trim_start_matches([' ', '=', '"']);
        let end = rest.find(['"', ',', ')']).unwrap_or(rest.len());
        let name = rest[..end].trim();
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out
}

/// Pull every `async <ident>(...)` defined on the Entrypoint class in
/// shim.js. Excludes `fetch` (handled by the wbg-generated path).
fn extract_shim_methods(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let l = line.trim();
        let rest = match l.strip_prefix("async ") {
            Some(r) => r,
            None => continue,
        };
        let end = match rest.find('(') {
            Some(i) => i,
            None => continue,
        };
        let name = rest[..end].trim();
        if name.is_empty() || name == "fetch" {
            continue;
        }
        // Plain identifier only -- skip arrow funcs, computed names, etc.
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.insert(name.to_string());
        }
    }
    out
}
