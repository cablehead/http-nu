# bench

oha-driven benchmark for the `cloudflare-shell-rpc` subsystem. Lives
inside the subsystem (`crates/cloudflare-shell-rpc/bench/`) for
locality with the things it measures.

Hits a running demo Worker (JS on :8789 or Rust on :8790) with
[`oha`](https://github.com/hatoo/oha); the demo forwards over the
FS-RPC service binding to the server + DurableObject + R2 Workspace.
Parses rps + latency, optionally appends to `results.nuon`.

Same shape as the top-level [`benchmarks/bench-cf/`](../../../benchmarks/bench-cf/)
plus a JS-vs-Rust pairing in the report so you can see the typed
Rust client's overhead vs. the JS-direct path.

## Local (requires the Workers running)

```bash
mise run cf:fs:up               # start server + demo-js + demo-rust
mise run cf:fs:bench:local      # bench both demos against a fixed matrix
mise run cf:fs:bench:sizes      # measure deployed bundle size per Worker
mise run cf:fs:bench:report     # render REPORT.md
mise run cf:fs:down             # tear down
```

## Remote (deployed Workers)

```bash
mise run cf:fs:bench:remote     # LIVE_BASE_JS / LIVE_BASE_RUST override
```

## Layout

- `run.nu` -- single-URL benchmark runner. Takes one path, runs oha,
  appends a row to `results.nuon`. Use this for ad-hoc benches.
- `matrix.nu` -- orchestrator. Calls `run.nu` once per row of the
  JS-vs-Rust path matrix. The `cf:fs:bench:local` / `:remote` tasks
  wrap this.
- `sizes.nu` -- measures deployed bundle size per Worker (server,
  demo-js, demo-rust) via `wrangler deploy --dry-run --outdir=...`.
  Writes one row per Worker to `sizes.nuon`. Wrapper:
  `mise run cf:fs:bench:sizes`. Tracks raw + gzip-9 + headroom against
  the 1 MB (self), 3 MB (free), 10 MB (paid) ceilings.
- `report.nu` -- renders `results.nuon` + `sizes.nuon` into `REPORT.md`,
  including the JS-vs-Rust delta column and the per-Worker size table.

## Direct invocation

```bash
# whole matrix against local demos
nu crates/cloudflare-shell-rpc/bench/matrix.nu

# single ad-hoc bench (won't go through the matrix)
nu crates/cloudflare-shell-rpc/bench/run.nu \
  --url http://127.0.0.1:8789 \
  --path /fs/bench/payload.bin \
  --duration 30s \
  --connections 100 \
  --seed-size 4096 \
  --save \
  --label "demo-js read 4KB"
```

## What gets benchmarked

The default matrix hits the same paths against both demos so the
report can show a JS-vs-Rust spread:

| Path                            | What it measures                                              |
|---------------------------------|---------------------------------------------------------------|
| `/`                             | Banner. Worker raw HTTP RPS, no RPC.                          |
| `/fs/bench/payload.bin`         | `readFile`: RPC + DO SQL lookup + base64 + body bytes.        |
| `/stat/bench/payload.bin`       | `stat`: RPC + DO SQL lookup.                                  |
| `/list/bench/`                  | `list`: RPC + DO SQL scan + serialize entries.                |

The bench seeds `payload.bin` once before reading, so GET paths
always find a file of the configured `--seed-size`.

## Numbers to interpret carefully

- **Local** runs wrangler dev's unoptimised dev-mode wasm in workerd
  on Node. Numbers reflect your laptop + Node + dev profile. Not
  representative of production.
- **Remote** is real CF edge. Varies by edge colo, time of day.
- **JS vs Rust**: the JS demo speaks JSON directly. The Rust demo
  wraps every call through `cloudflare-shell-rpc-client` (typed
  serde-wasm-bindgen). The delta tells you what that ergonomic
  wrapper costs.
- First request to a fresh DurableObject is slower (cold start).
  Steady-state rps (what oha reports) excludes that.

Requires `oha` in PATH (`cargo install oha` or `mise install`).
