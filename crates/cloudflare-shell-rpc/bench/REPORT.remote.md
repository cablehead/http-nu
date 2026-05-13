# cloudflare-shell-rpc/bench -- (remote)

> _Production rows -- requests against deployed Workers at_
> `*.gedw99.workers.dev`. _For the dev-mode counterpart see_
> [`REPORT.local.md`](./REPORT.local.md).

Auto-generated from `crates/cloudflare-shell-rpc/bench/results.nuon`. Regenerate this report with `mise run cf:fs:bench:remote && mise run cf:fs:bench:report`.

- Latest run captured: `2026-05-13T17:05:39`
- Total runs recorded: 36

## Latest snapshot per label

| label | requests_per_sec | avg_ms | p50_ms | p99_ms | ok_count | err_count | when |
| --- | --- | --- | --- | --- | --- | --- | --- |
| server banner (no binding) (remote) | 1123.86 | 44.58 | 43.54 | 70.21 | 11194 | 0 | 2026-05-13T17:01:46 |
| demo-rust banner (remote) | 1071.09 | 46.77 | 45.91 | 60.76 | 10667 | 0 | 2026-05-13T17:02:08 |
| demo-js banner (remote) | 1005.2 | 49.35 | 48.02 | 82.28 | 10005 | 0 | 2026-05-13T17:01:57 |
| server exists (remote) | 741.29 | 67.52 | 68.09 | 139.08 | 7367 | 0 | 2026-05-13T17:04:06 |
| demo-rust exists (remote) | 712.67 | 70.33 | 70.68 | 137.21 | 7081 | 0 | 2026-05-13T17:04:29 |
| demo-rust info (remote) | 685.55 | 73.35 | 75.17 | 121.48 | 6808 | 0 | 2026-05-13T17:05:39 |
| demo-js exists (remote) | 684.42 | 73.39 | 73.76 | 159.29 | 6797 | 0 | 2026-05-13T17:04:18 |
| server glob (remote) | 679.35 | 72.9 | 73.86 | 128.93 | 6746 | 0 | 2026-05-13T17:04:41 |
| server info (remote) | 676.78 | 74.16 | 74.57 | 144.84 | 6722 | 0 | 2026-05-13T17:05:15 |
| demo-rust glob (remote) | 672.27 | 74.6 | 74.78 | 164.78 | 6676 | 0 | 2026-05-13T17:05:04 |
| demo-rust read 1024B (remote) | 661.66 | 76 | 77.55 | 140.03 | 6570 | 0 | 2026-05-13T17:02:45 |
| server list (remote) | 656.32 | 76.56 | 77.53 | 117.51 | 6517 | 0 | 2026-05-13T17:03:31 |
| demo-rust stat (remote) | 654.9 | 75.69 | 76.76 | 150.72 | 6501 | 0 | 2026-05-13T17:03:19 |
| demo-rust list (remote) | 647.51 | 77.2 | 78.79 | 128.69 | 6428 | 0 | 2026-05-13T17:03:54 |
| demo-js list (remote) | 637.83 | 78.88 | 80.04 | 130 | 6331 | 0 | 2026-05-13T17:03:43 |
| demo-js glob (remote) | 637.02 | 78.9 | 78.84 | 132.14 | 6323 | 0 | 2026-05-13T17:04:52 |
| server stat (remote) | 636.12 | 78.99 | 78.47 | 150.02 | 6315 | 0 | 2026-05-13T17:02:56 |
| server read 1024B (remote) | 634.55 | 79.21 | 78.37 | 155.44 | 6299 | 0 | 2026-05-13T17:02:20 |
| demo-js info (remote) | 624.19 | 80.36 | 78.98 | 179.19 | 6194 | 1 | 2026-05-13T17:05:27 |
| demo-js stat (remote) | 619.63 | 81.15 | 81.51 | 168.83 | 6149 | 0 | 2026-05-13T17:03:08 |
| demo-js read 1024B (remote) | 599.14 | 84.03 | 83.31 | 165.89 | 5944 | 0 | 2026-05-13T17:02:32 |

## Three-tier comparison (latest per op)

Each operation is benched three ways:

- `server_rps` -- hit the FS-RPC server's HTTP routes directly. No
  service binding, no demo hop, no RPC dispatch -- just a worker
  responding to HTTP.
- `js_rps` -- hit the JS demo, which dispatches via the service
  binding RPC method. Cost over `server` = binding + RPC dispatch.
- `rust_rps` -- hit the Rust demo, which adds the typed
  `cloudflare-shell-rpc-client` wrapper on top of the RPC binding.

`js_vs_server_pct` shows the binding + RPC overhead (positive = faster
than direct, negative = slower). `rust_vs_js_pct` shows the cost of
the typed Rust client wrapper specifically.

| op | server_rps | js_rps | rust_rps | js_vs_server_pct | rust_vs_js_pct |
| --- | --- | --- | --- | --- | --- |
| exists (remote) | 741.29 | 684.42 | 712.67 | -7.7 | 4.1 |
| glob (remote) | 679.35 | 637.02 | 672.27 | -6.2 | 5.5 |
| info (remote) | 676.78 | 624.19 | 685.55 | -7.8 | 9.8 |
| list (remote) | 656.32 | 637.83 | 647.51 | -2.8 | 1.5 |
| read 1024B (remote) | 634.55 | 599.14 | 661.66 | -5.6 | 10.4 |
| stat (remote) | 636.12 | 619.63 | 654.9 | -2.6 | 5.7 |

## Analysis

Headline takeaways from the latest data above. Numbers update automatically when `cf:fs:bench:report` runs.

**No local data yet** -- run `mise run cf:fs:bench:all` to see local-dev relative deltas alongside remote numbers.

**Binding + RPC overhead.** `demo-js` vs `server` measures the cost of going through a service-binding RPC instead of hitting the server's HTTP route directly. A small or negative number means the binding hop is essentially free.
- Real edge median: **-5.9%**.

**Typed Rust client cost.** `demo-rust` vs `demo-js` isolates the `cloudflare-shell-rpc-client` wrapper (hand-written wasm-bindgen extern + `serde-wasm-bindgen` round-trip). Positive = Rust faster, negative = the wrapper is overhead.
- Real edge median: **+5.6%**.

The wrapper is essentially free for primitives. On big-response ops (`stat` / `list` with non-trivial JSON), the JS-side parses native; the Rust side does an extra `serde_wasm_bindgen::from_value`. Worth re-measuring if it ever pushes past ~30%.

**What single-run numbers do NOT tell you.** Each bench row is one ~3-10s oha sample. Same-op runs vary 5-20% across runs (wrangler dev sometimes more); single-row outliers (especially `(remote)` rows during peak edge load) shouldn't be over-fit. The **rolling averages** section below smooths this; the **history** section is the raw trail. For a defensible production claim, run `cf:fs:bench:remote` several times across different times of day and quote the median.

**When numbers are 0 / NaN.** The bench parser pulls `rps` / `avg` / `p99` from oha's text output. If `ok_count` is 0 but `rps` is non-zero, oha got responses but they weren't HTTP 2xx -- usually wrangler dev cracking under sustained load, or a deployed Worker hitting a rate-cap. Treat those rows as bench failures, not slow performance.

## Deployment sizes

Per-Worker bundle size at last measurement. `gz_total` is what CF charges
against the script-size limit; raw is the on-disk bundle before
compression. Budgets:

- **1 MB (self)** -- self-imposed for this subsystem (each Worker is meant
  to compose with others via service binding -- staying small is the point).
- **3 MB (free)** -- CF Workers free-plan ceiling.
- **10 MB (paid)** -- CF Workers paid-plan ceiling.

Regenerate via `mise run cf:fs:bench:sizes`.

| worker | raw_total | gz_total | vs 1MB (self) | vs 3MB (free) | vs 10MB (paid) | captured |
| --- | --- | --- | --- | --- | --- | --- |
| demo-js | 23.6 KB | 6.6 KB | 0.6% | 0.2% | 0.1% | 2026-05-13T16:54:44 |
| demo-rust | 558.4 KB | 201.5 KB | 19.7% | 6.6% | 2.0% | 2026-05-13T16:54:50 |
| server | 964.5 KB | 304.6 KB | 29.7% | 9.9% | 3.0% | 2026-05-13T16:54:43 |

## Rolling averages

How each label performs across every run we've captured.

| label | runs | avg_rps | avg_p50_ms | avg_p99_ms |
| --- | --- | --- | --- | --- |
| server banner (no binding) (remote) | 3 | 894.83 | 43.41 | 86.48 |
| demo-js banner (remote) | 3 | 817.49 | 47.16 | 92.76 |
| demo-rust banner (remote) | 3 | 779.45 | 30.47 | 65.66 |
| server exists (remote) | 1 | 741.29 | 68.09 | 139.08 |
| demo-rust exists (remote) | 1 | 712.67 | 70.68 | 137.21 |
| demo-rust info (remote) | 1 | 685.55 | 75.17 | 121.48 |
| demo-js exists (remote) | 1 | 684.42 | 73.76 | 159.29 |
| server glob (remote) | 1 | 679.35 | 73.86 | 128.93 |
| server info (remote) | 1 | 676.78 | 74.57 | 144.84 |
| demo-rust glob (remote) | 1 | 672.27 | 74.78 | 164.78 |
| demo-js glob (remote) | 1 | 637.02 | 78.84 | 132.14 |
| demo-js info (remote) | 1 | 624.19 | 78.98 | 179.19 |
| server list (remote) | 2 | 503.41 | 65.68 | 131.68 |
| server stat (remote) | 2 | 474.09 | 67.44 | 157.13 |
| demo-rust list (remote) | 2 | 473.21 | 70.65 | 142.68 |
| demo-rust read 1024B (remote) | 2 | 473 | 67.83 | 371.55 |
| demo-js list (remote) | 2 | 472.18 | 70.58 | 138.35 |
| demo-js stat (remote) | 2 | 470.59 | 69.82 | 156.66 |
| demo-rust stat (remote) | 2 | 461.08 | 68.75 | 264.34 |
| server read 1024B (remote) | 2 | 458.77 | 66.78 | 403.73 |
| demo-js read 1024B (remote) | 2 | 436.23 | 71.59 | 390.01 |

## Recent history -- last 20 rows

| when | label | requests_per_sec | p50_ms | p99_ms | ok_count | err_count |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-05-13T17:05:39 | demo-rust info (remote) | 685.55 | 75.17 | 121.48 | 6808 | 0 |
| 2026-05-13T17:05:27 | demo-js info (remote) | 624.19 | 78.98 | 179.19 | 6194 | 1 |
| 2026-05-13T17:05:15 | server info (remote) | 676.78 | 74.57 | 144.84 | 6722 | 0 |
| 2026-05-13T17:05:04 | demo-rust glob (remote) | 672.27 | 74.78 | 164.78 | 6676 | 0 |
| 2026-05-13T17:04:52 | demo-js glob (remote) | 637.02 | 78.84 | 132.14 | 6323 | 0 |
| 2026-05-13T17:04:41 | server glob (remote) | 679.35 | 73.86 | 128.93 | 6746 | 0 |
| 2026-05-13T17:04:29 | demo-rust exists (remote) | 712.67 | 70.68 | 137.21 | 7081 | 0 |
| 2026-05-13T17:04:18 | demo-js exists (remote) | 684.42 | 73.76 | 159.29 | 6797 | 0 |
| 2026-05-13T17:04:06 | server exists (remote) | 741.29 | 68.09 | 139.08 | 7367 | 0 |
| 2026-05-13T17:03:54 | demo-rust list (remote) | 647.51 | 78.79 | 128.69 | 6428 | 0 |
| 2026-05-13T17:03:43 | demo-js list (remote) | 637.83 | 80.04 | 130 | 6331 | 0 |
| 2026-05-13T17:03:31 | server list (remote) | 656.32 | 77.53 | 117.51 | 6517 | 0 |
| 2026-05-13T17:03:19 | demo-rust stat (remote) | 654.9 | 76.76 | 150.72 | 6501 | 0 |
| 2026-05-13T17:03:08 | demo-js stat (remote) | 619.63 | 81.51 | 168.83 | 6149 | 0 |
| 2026-05-13T17:02:56 | server stat (remote) | 636.12 | 78.47 | 150.02 | 6315 | 0 |
| 2026-05-13T17:02:45 | demo-rust read 1024B (remote) | 661.66 | 77.55 | 140.03 | 6570 | 0 |
| 2026-05-13T17:02:32 | demo-js read 1024B (remote) | 599.14 | 83.31 | 165.89 | 5944 | 0 |
| 2026-05-13T17:02:20 | server read 1024B (remote) | 634.55 | 78.37 | 155.44 | 6299 | 0 |
| 2026-05-13T17:02:08 | demo-rust banner (remote) | 1071.09 | 45.91 | 60.76 | 10667 | 0 |
| 2026-05-13T17:01:57 | demo-js banner (remote) | 1005.2 | 48.02 | 82.28 | 10005 | 0 |

---

Notes:
- The JS demo speaks JSON to the binding directly. The Rust demo goes
  through the typed `cloudflare-shell-rpc-client` wrapper (serde-wasm-bindgen
  encode/decode). The `rust_vs_js_pct` column isolates that overhead.
- Cold start adds latency to the first request after a fresh DO isolate.
- The seed step runs once before each bench so GET /fs paths always read
  a file of the configured size.
