# cloudflare-shell-rpc-demo-rust

Rust Worker that consumes `cloudflare-shell-rpc` via
[`cloudflare-shell-rpc-client`](../client/) and exposes curl-able HTTP
routes mirroring `demo-js`'s surface. Two purposes:

1. **Rust-consumer reference.** Minimal `wrangler.toml` + service-binding
   wiring + typed `env.service("SHELL_FS")?.into()` usage. ~140 LoC.
2. **Integration test for `client/`.** The `client` crate is
   hand-written wasm-bindgen ceremony; there's no way to unit-test it.
   `cf:fs:smoke:rust` runs the same round-trip sequence as
   `cf:fs:smoke` (the JS smoke) against this Worker. Both passing
   means the client crate works end-to-end.

## Routes

Same shape as `demo-js` so `cf:fs:smoke:rust` can reuse the test
sequence:

| Method | Path                  | RPC call         |
|--------|-----------------------|------------------|
| GET    | `/`                   | banner           |
| GET    | `/fs/:ns/:path`       | `read_file`      |
| PUT    | `/fs/:ns/:path`       | `write_file`     |
| DELETE | `/fs/:ns/:path`       | `rm`             |
| GET    | `/stat/:ns/:path`     | `stat`           |
| GET    | `/list/:ns/:path`     | `list`           |
| POST   | `/mkdir/:ns/:path`    | `mkdir`          |

## Build / deploy

```bash
mise run cf:fs:demo:rust:build
mise run cf:fs:demo:rust:dev     # wrangler dev on :8790 (server on :8788)
mise run cf:fs:demo:rust:deploy
```

## Smoke test

```bash
mise run cf:fs:smoke:rust        # curls http://127.0.0.1:8790 by default
```
