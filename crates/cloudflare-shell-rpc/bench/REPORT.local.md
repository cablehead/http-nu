# cloudflare-shell-rpc/bench -- (local)

> ## ⚠️ DEV NUMBERS, NOT PRODUCTION
>
> These rows come from `wrangler dev`: unoptimised wasm, debug profile,
> hosted in workerd-on-Node. They are useful for spotting **regressions**
> and JS-vs-Rust **relative** differences but are **not** representative
> of production rps / latency. The deployed-Worker numbers live in
> [`REPORT.remote.md`](./REPORT.remote.md).

Auto-generated from `crates/cloudflare-shell-rpc/bench/results.nuon`. Regenerate this report with `mise run cf:fs:bench:local && mise run cf:fs:bench:report`.

- Latest run captured: `2026-05-13T16:54:23`
- Total runs recorded: 43

## Latest snapshot per label

| label | requests_per_sec | avg_ms | p50_ms | p99_ms | ok_count | err_count | when |
| --- | --- | --- | --- | --- | --- | --- | --- |
| demo-rust banner | 3182.26 | 6.27 | 5.63 | 16.87 | 9535 | 0 | 2026-05-13T16:53:06 |
| server banner (no binding) | 3113.31 | 6.42 | 5.69 | 17.11 | 9335 | 0 | 2026-05-13T16:52:57 |
| demo-rust list | 2080.44 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T16:53:44 |
| server exists | 1992.69 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T16:53:48 |
| demo-js banner | 1913.6 | 10.45 | 10 | 18.3 | 5728 | 0 | 2026-05-13T16:53:01 |
| demo-js read 1024B | 1837.39 | 10.88 | 9.71 | 31.08 | 5496 | 0 | 2026-05-13T16:53:14 |
| demo-js stat | 1816.53 | 11.02 | 9.36 | 35 | 5435 | 0 | 2026-05-13T16:53:27 |
| demo-rust read 1024B | 1676.65 | 11.95 | 10.62 | 37.59 | 5015 | 0 | 2026-05-13T16:53:18 |
| demo-js info | 1654.34 | 11.76 | 9 | 60.31 | 4899 | 51 | 2026-05-13T16:54:18 |
| server glob | 1575.94 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T16:54:01 |
| demo-js exists | 1492.52 | 159.5 | 0.05 | 2.08 | 21 | 234 | 2026-05-13T16:53:53 |
| server read 1024B | 1441.23 | 13.9 | 12.18 | 35.83 | 4309 | 0 | 2026-05-13T16:53:10 |
| server stat | 1337.31 | 14.83 | 12.43 | 66.78 | 3996 | 0 | 2026-05-13T16:53:23 |
| demo-rust glob | 1328.09 | 15.09 | 12.24 | 53.69 | 3970 | 0 | 2026-05-13T16:54:10 |
| server info | 1250.62 | 16.03 | 14.1 | 47.16 | 3735 | 0 | 2026-05-13T16:54:14 |
| server list | 1200.01 | 16.71 | 14 | 70.3 | 3582 | 0 | 2026-05-13T16:53:36 |
| demo-js glob | 896.26 | 30.7 | 11.92 | 138.19 | 1210 | 270 | 2026-05-13T16:54:05 |
| demo-rust info | 894.68 | 0 | 0 | 0 | 0 | 0 | 2026-05-13T16:54:23 |
| demo-rust exists | 871.63 | 57.61 | 58.96 | 109.02 | 50 | 580 | 2026-05-13T16:53:57 |
| demo-js list | 756.56 | 26.45 | 15.82 | 77.05 | 1300 | 952 | 2026-05-13T16:53:40 |
| demo-rust stat | 737.83 | 27.3 | 26.37 | 60.74 | 1984 | 212 | 2026-05-13T16:53:31 |

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
| exists | 1992.69 | 1492.52 | 871.63 | -25.1 | -41.6 |
| glob | 1575.94 | 896.26 | 1328.09 | -43.1 | 48.2 |
| info | 1250.62 | 1654.34 | 894.68 | 32.3 | -45.9 |
| list | 1200.01 | 756.56 | 2080.44 | -37 | 175 |
| read 1024B | 1441.23 | 1837.39 | 1676.65 | 27.5 | -8.7 |
| stat | 1337.31 | 1816.53 | 737.83 | 35.8 | -59.4 |

## Analysis

Headline takeaways from the latest data above. Numbers update automatically when `cf:fs:bench:report` runs.

**No remote data yet** -- run `mise run cf:fs:bench:remote` against deployed Workers to compare dev vs. real-edge numbers. Local rows alone give relative deltas, not absolute production performance.

**Binding + RPC overhead.** `demo-js` vs `server` measures the cost of going through a service-binding RPC instead of hitting the server's HTTP route directly. A small or negative number means the binding hop is essentially free.
- Local-dev median: **+1.2%**.

**Typed Rust client cost.** `demo-rust` vs `demo-js` isolates the `cloudflare-shell-rpc-client` wrapper (hand-written wasm-bindgen extern + `serde-wasm-bindgen` round-trip). Positive = Rust faster, negative = the wrapper is overhead.
- Local-dev median: **-25.2%**.

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
| server banner (no binding) | 4 | 3222.85 | 7.54 | 22.76 |
| demo-rust banner | 4 | 3213.34 | 7.59 | 22.16 |
| demo-js banner | 4 | 2264.88 | 13.02 | 27.24 |
| server exists | 1 | 1992.69 | 0 | 0 |
| demo-js info | 1 | 1654.34 | 9 | 60.31 |
| demo-js stat | 2 | 1611.47 | 7.26 | 39.82 |
| server glob | 1 | 1575.94 | 0 | 0 |
| demo-rust read 1024B | 2 | 1557.84 | 8.16 | 35.95 |
| demo-js exists | 1 | 1492.52 | 0.05 | 2.08 |
| server read 1024B | 4 | 1444.31 | 16.85 | 46.58 |
| demo-rust glob | 1 | 1328.09 | 12.24 | 53.69 |
| demo-js read 1024B | 4 | 1298.41 | 14.6 | 180.02 |
| server stat | 2 | 1275.66 | 9.47 | 53.86 |
| server info | 1 | 1250.62 | 14.1 | 47.16 |
| demo-rust list | 2 | 1217.8 | 5.95 | 53.02 |
| server list | 2 | 1057.74 | 7 | 35.15 |
| demo-js glob | 1 | 896.26 | 11.92 | 138.19 |
| demo-rust info | 1 | 894.68 | 0 | 0 |
| demo-rust exists | 1 | 871.63 | 58.96 | 109.02 |
| demo-rust stat | 2 | 704.97 | 13.19 | 30.37 |
| demo-js list | 2 | 555.19 | 37.96 | 90.28 |

## Recent history -- last 20 rows

| when | label | requests_per_sec | p50_ms | p99_ms | ok_count | err_count |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-05-13T16:54:23 | demo-rust info | 894.68 | 0 | 0 | 0 | 0 |
| 2026-05-13T16:54:18 | demo-js info | 1654.34 | 9 | 60.31 | 4899 | 51 |
| 2026-05-13T16:54:14 | server info | 1250.62 | 14.1 | 47.16 | 3735 | 0 |
| 2026-05-13T16:54:10 | demo-rust glob | 1328.09 | 12.24 | 53.69 | 3970 | 0 |
| 2026-05-13T16:54:05 | demo-js glob | 896.26 | 11.92 | 138.19 | 1210 | 270 |
| 2026-05-13T16:54:01 | server glob | 1575.94 | 0 | 0 | 0 | 0 |
| 2026-05-13T16:53:57 | demo-rust exists | 871.63 | 58.96 | 109.02 | 50 | 580 |
| 2026-05-13T16:53:53 | demo-js exists | 1492.52 | 0.05 | 2.08 | 21 | 234 |
| 2026-05-13T16:53:48 | server exists | 1992.69 | 0 | 0 | 0 | 0 |
| 2026-05-13T16:53:44 | demo-rust list | 2080.44 | 0 | 0 | 0 | 0 |
| 2026-05-13T16:53:40 | demo-js list | 756.56 | 15.82 | 77.05 | 1300 | 952 |
| 2026-05-13T16:53:36 | server list | 1200.01 | 14 | 70.3 | 3582 | 0 |
| 2026-05-13T16:53:31 | demo-rust stat | 737.83 | 26.37 | 60.74 | 1984 | 212 |
| 2026-05-13T16:53:27 | demo-js stat | 1816.53 | 9.36 | 35 | 5435 | 0 |
| 2026-05-13T16:53:23 | server stat | 1337.31 | 12.43 | 66.78 | 3996 | 0 |
| 2026-05-13T16:53:18 | demo-rust read 1024B | 1676.65 | 10.62 | 37.59 | 5015 | 0 |
| 2026-05-13T16:53:14 | demo-js read 1024B | 1837.39 | 9.71 | 31.08 | 5496 | 0 |
| 2026-05-13T16:53:10 | server read 1024B | 1441.23 | 12.18 | 35.83 | 4309 | 0 |
| 2026-05-13T16:53:06 | demo-rust banner | 3182.26 | 5.63 | 16.87 | 9535 | 0 |
| 2026-05-13T16:53:01 | demo-js banner | 1913.6 | 10 | 18.3 | 5728 | 0 |

---

Notes:
- The JS demo speaks JSON to the binding directly. The Rust demo goes
  through the typed `cloudflare-shell-rpc-client` wrapper (serde-wasm-bindgen
  encode/decode). The `rust_vs_js_pct` column isolates that overhead.
- Cold start adds latency to the first request after a fresh DO isolate.
- The seed step runs once before each bench so GET /fs paths always read
  a file of the configured size.
