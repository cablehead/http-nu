#!/usr/bin/env nu

# Render benchmarks/bench-cf/results.nuon into a markdown report.
#
# Usage:
#   nu benchmarks/bench-cf/report.nu                  # print to stdout
#   nu benchmarks/bench-cf/report.nu --save           # write REPORT.md
#
# The report has three sections:
#   1) Latest run per (target, path) -- the freshest snapshot
#   2) Per-path rolling average (rps + p50 + p99 across all runs)
#   3) Full history -- every recorded run, newest first
#
# Linked from CLOUDFLARE_STATUS.md so the live worker's measured
# performance is one click from the status doc.

def md-table [rows: list] {
  if ($rows | is-empty) {
    return "_(no data)_\n"
  }
  let cols = ($rows | first | columns)
  let header = "| " + ($cols | str join " | ") + " |"
  let sep = "| " + ($cols | each {|_| "---" } | str join " | ") + " |"
  let body = ($rows | each {|r|
    let cells = ($cols | each {|c| $r | get $c | into string } | str join " | ")
    $"| ($cells) |"
  } | str join "\n")
  $"($header)\n($sep)\n($body)\n"
}

def main [
  --save (-s)        # write to REPORT.md instead of stdout
] {
  let script_dir = ($env.FILE_PWD? | default ".")
  let data_path = $"($script_dir)/results.nuon"

  if not ($data_path | path exists) {
    print $"✗ no results yet -- run `mise run cf:bench:local` or `cf:bench:remote` first"
    exit 1
  }

  let rows = open $data_path
  let total = ($rows | length)

  # Section 1: latest per (target, path)
  let latest = ($rows
    | group-by label
    | items {|k v| $v | sort-by when | last }
    | sort-by requests_per_sec --reverse
    | select label requests_per_sec avg_ms p50_ms p99_ms ok_count err_count when)

  # Section 2: rolling average per label
  let rolling = ($rows
    | group-by label
    | items {|k v| {
        label: $k
        runs: ($v | length)
        avg_rps: (($v | get requests_per_sec | math avg) | math round -p 2)
        avg_p50_ms: (($v | get p50_ms | math avg) | math round -p 2)
        avg_p99_ms: (($v | get p99_ms | math avg) | math round -p 2)
      }}
    | sort-by avg_rps --reverse)

  # Section 3: full history (newest first)
  let history = ($rows
    | sort-by when --reverse
    | select when label requests_per_sec p50_ms p99_ms ok_count err_count
    | first 20)

  let when_latest = ($rows | sort-by when | last | get when)
  let header = $"# bench-cf -- results report

Auto-generated from `benchmarks/bench-cf/results.nuon`. Regenerate via `mise run cf:bench:report`.

- Latest run captured: `($when_latest)`
- Total runs recorded: ($total)

## Latest snapshot per target

"
  let s2 = "
## Rolling averages

How each target performs across every run we've captured.

"
  let s3 = "
## Recent history -- last 20 rows

"
  let footer = "
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
"
  let report = $header + (md-table $latest) + $s2 + (md-table $rolling) + $s3 + (md-table $history) + $footer

  if $save {
    let out_path = $"($script_dir)/REPORT.md"
    $report | save -f $out_path
    print $"✓ wrote ($out_path)"
  } else {
    print $report
  }
}
