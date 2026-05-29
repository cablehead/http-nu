# bench-cf

A url-driven benchmark for http-nu Workers. Hits a running server with
[`oha`](https://github.com/hatoo/oha), parses rps + latency, optionally
appends to `results.nuon`.

Doesn't spin up the server itself -- that's mise's job:

```bash
# local wrangler dev
mise run cf:dev:hub          # terminal 1
mise run cf:bench:local      # terminal 2

# live worker
mise run cf:bench:remote
```

Direct invocation:

```bash
nu benchmarks/bench-cf/run.nu \
  --url https://http-nu-cf.gedw99.workers.dev \
  --path /datastar-counter/ \
  --duration 30s \
  --connections 100 \
  --save \
  --label "datastar-counter @ 100c"
```

Numbers to interpret carefully:
- **Local** runs wrangler dev's unoptimised dev-mode wasm hosted in Node.
  Numbers reflect your laptop + Node + dev profile. Not a production read.
- **Remote** is the real CF edge -- closer to a production read but
  varies by edge colo (proximity to your test machine), time of day,
  and any caching.
- First request to a fresh DurableObject is slower (cold start + the
  per-request workspace snapshot preload).

Requires `oha` in PATH. `cargo install oha` if missing.
