# cloudflare-shell-rpc-types

Wire types for the `cloudflare-shell-rpc` Worker. serde structs that
travel between the RPC server (`cloudflare-shell-rpc`) and its Rust
client wrapper (`cloudflare-shell-rpc-client`).

Pure Rust -- no `worker` dependency. Compiles on desktop too; that's
the point: serialization can be unit-tested without a wasm toolchain.

The JSON shape these structs serialize to is the **interop contract**
with JS consumers (which speak the same shape via `await
env.SHELL_FS.readFile({ ... })`). Treat field renames as breaking
changes.

See the parent crate's
[`README.md`](https://github.com/joeblew999/http-nu/tree/joeblew999/crates/cloudflare-shell-rpc)
for the overall story.

## License

MIT.
