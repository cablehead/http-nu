# cargo-docs

Serve `cargo doc` output with a generated index page listing all crates.

## Usage

```bash
cargo doc --workspace --no-deps
http-nu :3001 examples/cargo-docs/serve.nu
```

Point at docs in a different location:

```nu
with-env {DOC_ROOT: "/path/to/target/doc"} {|| http-nu :3001 examples/cargo-docs/serve.nu }
```

Defaults to `./target/doc` when `DOC_ROOT` is not set.
