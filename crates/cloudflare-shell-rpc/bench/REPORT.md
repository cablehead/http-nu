# cloudflare-shell-rpc/bench -- results report

> ## ⚠️ DEV NUMBERS, NOT PRODUCTION
>
> Rows with targets at `127.0.0.1` come from `wrangler dev`: unoptimised
> wasm, debug profile, hosted in workerd-on-Node. They are useful for
> spotting **regressions** and JS-vs-Rust **relative** differences but
> are **not** representative of production rps / latency on the real
> Cloudflare edge. For prod numbers run `mise run cf:fs:bench:remote`
> against a deployed Worker.

Auto-generated from `crates/cloudflare-shell-rpc/bench/results.nuon`. Regenerate via `mise run cf:fs:bench:report`.

- Latest run captured: `2026-05-13T15:09:06`
- Total runs recorded: 24

## Latest snapshot per label

| label | requests_per_sec | avg_ms | p50_ms | p99_ms | ok_count | err_count | when |
| --- | --- | --- | --- | --- | --- | --- | --- |
| demo-js banner | 3274.93 | 3.05 | 2.64 | 13.48 | 16374 | 0 | 2026-05-13T14:54:39 |
| server banner (no binding) | 3227.26 | 3.09 | 2.66 | 13.29 | 16145 | 0 | 2026-05-13T14:54:33 |
| demo-rust banner | 3192.04 | 3.13 | 2.69 | 13.16 | 15959 | 0 | 2026-05-13T14:54:46 |
| demo-js read 1024B | 1679.05 | 5.96 | 4.93 | 23.98 | 8389 | 0 | 2026-05-13T14:54:58 |
| server read 1024B | 1458.72 | 6.86 | 5.79 | 24.07 | 7287 | 0 | 2026-05-13T14:54:52 |
| demo-rust read 1024B | 1439.02 | 6.95 | 5.71 | 34.31 | 7188 | 0 | 2026-05-13T14:55:04 |
| demo-js stat | 1406.41 | 7.06 | 5.16 | 44.64 | 6986 | 42 | 2026-05-13T14:55:17 |
| server stat | 1214.01 | 8.24 | 6.51 | 40.94 | 6063 | 0 | 2026-05-13T14:55:11 |
| server list | 915.47 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T14:55:30 |
| demo-rust stat | 672.11 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T14:55:23 |
| server banner (no binding) (remote) | 434.35 | 46.19 | 43.27 | 129.04 | 1285 | 0 | 2026-05-13T15:08:13 |
| demo-js banner (remote) | 425.69 | 47.32 | 45.33 | 131.29 | 1259 | 0 | 2026-05-13T15:08:17 |
| demo-rust banner (remote) | 422.15 | 47.72 | 45.44 | 136.07 | 1248 | 0 | 2026-05-13T15:08:21 |
| demo-rust list | 355.16 | 33.54 | 11.9 | 106.04 | 770 | 422 | 2026-05-13T14:55:42 |
| demo-js list | 353.82 | 50.37 | 60.09 | 103.5 | 202 | 551 | 2026-05-13T14:55:36 |
| server list (remote) | 350.5 | 57.59 | 53.83 | 145.84 | 1033 | 0 | 2026-05-13T15:08:57 |
| demo-js stat (remote) | 321.55 | 63.05 | 58.12 | 144.49 | 946 | 0 | 2026-05-13T15:08:48 |
| server stat (remote) | 312.05 | 64.77 | 56.4 | 164.24 | 917 | 0 | 2026-05-13T15:08:43 |
| demo-js list (remote) | 306.52 | 65.93 | 61.11 | 146.69 | 901 | 0 | 2026-05-13T15:09:02 |
| demo-rust list (remote) | 298.91 | 67.6 | 62.5 | 156.67 | 878 | 0 | 2026-05-13T15:09:06 |
| demo-rust read 1024B (remote) | 284.33 | 71.18 | 58.11 | 603.07 | 834 | 0 | 2026-05-13T15:08:38 |
| server read 1024B (remote) | 282.98 | 71.59 | 55.19 | 652.02 | 830 | 0 | 2026-05-13T15:08:27 |
| demo-js read 1024B (remote) | 273.31 | 74.09 | 59.87 | 614.13 | 801 | 0 | 2026-05-13T15:08:33 |
| demo-rust stat (remote) | 267.26 | 75.15 | 60.74 | 377.96 | 783 | 0 | 2026-05-13T15:08:53 |

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
| list | 915.47 | 353.82 | 355.16 | -61.4 | 0.4 |
| list (remote) | 350.5 | 306.52 | 298.91 | -12.5 | -2.5 |
| read 1024B | 1458.72 | 1679.05 | 1439.02 | 15.1 | -14.3 |
| read 1024B (remote) | 282.98 | 273.31 | 284.33 | -3.4 | 4 |
| stat | 1214.01 | 1406.41 | 672.11 | 15.8 | -52.2 |
| stat (remote) | 312.05 | 321.55 | 267.26 | 3 | -16.9 |

## Analysis

Headline takeaways from the latest data above. Numbers update automatically when `cf:fs:bench:report` runs.

**Dev vs. real edge.** Local `wrangler dev` reports server-direct throughput around 1214.0 rps; the deployed Worker at the same op manages 312.0 rps -- the dev numbers are ~3.9x higher because wrangler dev runs on your laptop without real network RTT. Quote remote rows when comparing to production; quote local rows only for relative deltas and regression-spotting.

**Binding + RPC overhead.** `demo-js` vs `server` measures the cost of going through a service-binding RPC instead of hitting the server's HTTP route directly. A small or negative number means the binding hop is essentially free.
- Local-dev median: **+15.1%**.
- Real edge median: **-3.4%**.

If the local and remote numbers disagree wildly (e.g. local shows -60% on list, remote shows -12%), trust the remote -- local-dev's binding implementation is single-process workerd-on-Node, not what production runs.

**Typed Rust client cost.** `demo-rust` vs `demo-js` isolates the `cloudflare-shell-rpc-client` wrapper (hand-written wasm-bindgen extern + `serde-wasm-bindgen` round-trip). Positive = Rust faster, negative = the wrapper is overhead.
- Local-dev median: **-14.3%**.
- Real edge median: **-2.5%**.

The wrapper is essentially free for primitives. On big-response ops (`stat` / `list` with non-trivial JSON), the JS-side parses native; the Rust side does an extra `serde_wasm_bindgen::from_value`. Worth re-measuring if it ever pushes past ~30%.

**What single-run numbers do NOT tell you.** Each bench row is one ~3-10s oha sample. Same-op runs vary 5-20% across runs (wrangler dev sometimes more); single-row outliers (especially `(remote)` rows during peak edge load) shouldn't be over-fit. The **rolling averages** section below smooths this; the **history** section is the raw trail. For a defensible production claim, run `cf:fs:bench:remote` several times across different times of day and quote the median.

**When numbers are 0 / NaN.** The bench parser pulls `rps` / `avg` / `p99` from oha's text output. If `ok_count` is 0 but `rps` is non-zero, oha got responses but they weren't HTTP 2xx -- usually wrangler dev cracking under sustained load, or a deployed Worker hitting a rate-cap. Treat those rows as bench failures, not slow performance.

## Rolling averages

How each label performs across every run we've captured.

| label | runs | avg_rps | avg_p50_ms | avg_p99_ms |
| --- | --- | --- | --- | --- |
| demo-js banner | 1 | 3274.93 | 2.64 | 13.48 |
| server banner (no binding) | 1 | 3227.26 | 2.66 | 13.29 |
| demo-rust banner | 1 | 3192.04 | 2.69 | 13.16 |
| demo-js read 1024B | 1 | 1679.05 | 4.93 | 23.98 |
| server read 1024B | 1 | 1458.72 | 5.79 | 24.07 |
| demo-rust read 1024B | 1 | 1439.02 | 5.71 | 34.31 |
| demo-js stat | 1 | 1406.41 | 5.16 | 44.64 |
| server stat | 1 | 1214.01 | 6.51 | 40.94 |
| server list | 1 | 915.47 | 0 | 0 |
| demo-rust stat | 1 | 672.11 | 0 | 0 |
| server banner (no binding) (remote) | 1 | 434.35 | 43.27 | 129.04 |
| demo-js banner (remote) | 1 | 425.69 | 45.33 | 131.29 |
| demo-rust banner (remote) | 1 | 422.15 | 45.44 | 136.07 |
| demo-rust list | 1 | 355.16 | 11.9 | 106.04 |
| demo-js list | 1 | 353.82 | 60.09 | 103.5 |
| server list (remote) | 1 | 350.5 | 53.83 | 145.84 |
| demo-js stat (remote) | 1 | 321.55 | 58.12 | 144.49 |
| server stat (remote) | 1 | 312.05 | 56.4 | 164.24 |
| demo-js list (remote) | 1 | 306.52 | 61.11 | 146.69 |
| demo-rust list (remote) | 1 | 298.91 | 62.5 | 156.67 |
| demo-rust read 1024B (remote) | 1 | 284.33 | 58.11 | 603.07 |
| server read 1024B (remote) | 1 | 282.98 | 55.19 | 652.02 |
| demo-js read 1024B (remote) | 1 | 273.31 | 59.87 | 614.13 |
| demo-rust stat (remote) | 1 | 267.26 | 60.74 | 377.96 |

## Recent history -- last 20 rows

| when | label | requests_per_sec | p50_ms | p99_ms | ok_count | err_count |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-05-13T15:09:06 | demo-rust list (remote) | 298.91 | 62.5 | 156.67 | 878 | 0 |
| 2026-05-13T15:09:02 | demo-js list (remote) | 306.52 | 61.11 | 146.69 | 901 | 0 |
| 2026-05-13T15:08:57 | server list (remote) | 350.5 | 53.83 | 145.84 | 1033 | 0 |
| 2026-05-13T15:08:53 | demo-rust stat (remote) | 267.26 | 60.74 | 377.96 | 783 | 0 |
| 2026-05-13T15:08:48 | demo-js stat (remote) | 321.55 | 58.12 | 144.49 | 946 | 0 |
| 2026-05-13T15:08:43 | server stat (remote) | 312.05 | 56.4 | 164.24 | 917 | 0 |
| 2026-05-13T15:08:38 | demo-rust read 1024B (remote) | 284.33 | 58.11 | 603.07 | 834 | 0 |
| 2026-05-13T15:08:33 | demo-js read 1024B (remote) | 273.31 | 59.87 | 614.13 | 801 | 0 |
| 2026-05-13T15:08:27 | server read 1024B (remote) | 282.98 | 55.19 | 652.02 | 830 | 0 |
| 2026-05-13T15:08:21 | demo-rust banner (remote) | 422.15 | 45.44 | 136.07 | 1248 | 0 |
| 2026-05-13T15:08:17 | demo-js banner (remote) | 425.69 | 45.33 | 131.29 | 1259 | 0 |
| 2026-05-13T15:08:13 | server banner (no binding) (remote) | 434.35 | 43.27 | 129.04 | 1285 | 0 |
| 2026-05-13T14:55:42 | demo-rust list | 355.16 | 11.9 | 106.04 | 770 | 422 |
| 2026-05-13T14:55:36 | demo-js list | 353.82 | 60.09 | 103.5 | 202 | 551 |
| 2026-05-13T14:55:30 | server list | 915.47 | 0 | 0 | 0 | 0 |
| 2026-05-13T14:55:23 | demo-rust stat | 672.11 | 0 | 0 | 0 | 0 |
| 2026-05-13T14:55:17 | demo-js stat | 1406.41 | 5.16 | 44.64 | 6986 | 42 |
| 2026-05-13T14:55:11 | server stat | 1214.01 | 6.51 | 40.94 | 6063 | 0 |
| 2026-05-13T14:55:04 | demo-rust read 1024B | 1439.02 | 5.71 | 34.31 | 7188 | 0 |
| 2026-05-13T14:54:58 | demo-js read 1024B | 1679.05 | 4.93 | 23.98 | 8389 | 0 |

---

How to add more data:

```bash
# bring up the three Workers via pitchfork
mise run cf:fs:up

# benchmark a path against both demos
mise run cf:fs:bench:local

# regenerate this report
mise run cf:fs:bench:report

# tear down
mise run cf:fs:down
```

Notes:
- Local numbers reflect wrangler dev's unoptimised wasm in workerd-on-Node.
  Not a production read.
- The JS demo speaks JSON to the binding directly. The Rust demo goes
  through the typed `cloudflare-shell-rpc-client` wrapper (serde-wasm-bindgen
  encode/decode). The `rust_vs_js_pct` column isolates that overhead.
- Cold start adds latency to the first request after a fresh DO isolate.
- The seed step runs once before each bench so GET /fs paths always read
  a file of the configured size.
