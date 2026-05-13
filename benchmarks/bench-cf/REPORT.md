# bench-cf -- results report

Auto-generated from `benchmarks/bench-cf/results.nuon`. Regenerate via `mise run cf:bench:report`.

- Latest run captured: `2026-05-13T12:49:20`
- Total runs recorded: 6

## Latest snapshot per target

| label | requests_per_sec | avg_ms | p50_ms | p99_ms | ok_count | err_count | when |
| --- | --- | --- | --- | --- | --- | --- | --- |
| remote: /basic/hello | 342.54 | 147.1 | 141.95 | 288.24 | 1664 | 0 | 2026-05-13T12:49:15 |
| remote: /datastar-counter/ | 172.6 | 295.19 | 295.87 | 411.3 | 814 | 0 | 2026-05-13T12:49:20 |
| remote: / | 81.14 | 674.3 | 0.61 | 1.53 | 356 | 0 | 2026-05-13T12:49:10 |

## Rolling averages

How each target performs across every run we've captured.

| label | runs | avg_rps | avg_p50_ms | avg_p99_ms |
| --- | --- | --- | --- | --- |
| remote: /basic/hello | 2 | 343.95 | 140.54 | 306.41 |
| remote: /datastar-counter/ | 2 | 175.23 | 286.63 | 431.18 |
| remote: / | 2 | 82.6 | 346.91 | 488.18 |

## Recent history -- last 20 rows

| when | label | requests_per_sec | p50_ms | p99_ms | ok_count | err_count |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-05-13T12:49:20 | remote: /datastar-counter/ | 172.6 | 295.87 | 411.3 | 814 | 0 |
| 2026-05-13T12:49:15 | remote: /basic/hello | 342.54 | 141.95 | 288.24 | 1664 | 0 |
| 2026-05-13T12:49:10 | remote: / | 81.14 | 0.61 | 1.53 | 356 | 0 |
| 2026-05-13T12:48:42 | remote: /datastar-counter/ | 177.85 | 277.39 | 451.06 | 1018 | 0 |
| 2026-05-13T12:48:36 | remote: /basic/hello | 345.36 | 139.14 | 324.59 | 2024 | 0 |
| 2026-05-13T12:48:29 | remote: / | 84.05 | 693.2 | 974.82 | 455 | 0 |

---

How to add more data:

```bash
# benchmark the live worker
PATH_ARG=/basic/hello mise run cf:bench:remote

# or a specific demo
PATH_ARG=/datastar-counter/ DURATION=20s CONNECTIONS=100 mise run cf:bench:remote

# regenerate this report
mise run cf:bench:report
```

Notes on what to trust:
- Numbers are sensitive to your network proximity to the CF edge colo
  (live target). p99 jumps when the test machine is far from the edge.
- Local numbers reflect wrangler dev's unoptimised dev-mode wasm; not
  representative of production.
- Cold-start adds ~100-500ms to the first request after a fresh DO
  isolate. Steady-state rps (the value reported here) excludes that.
