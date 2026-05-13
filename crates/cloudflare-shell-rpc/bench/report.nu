#!/usr/bin/env nu

# Render crates/cloudflare-shell-rpc/bench/results.nuon into a markdown report.
#
# Same shape as benchmarks/bench-cf/report.nu (latest snapshot,
# rolling averages, recent history). Adds a JS-vs-Rust side-by-side
# section for paired labels like "demo-js read 1KB" / "demo-rust read 1KB".
#
# Usage:
#   nu crates/cloudflare-shell-rpc/bench/report.nu             # stdout
#   nu crates/cloudflare-shell-rpc/bench/report.nu --save      # write REPORT.local.md + REPORT.remote.md

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

# Round to integer (math round -p 0) for prose readability.
def fmt-pct [v: float] {
  if $v >= 0 { $"+($v | math round -p 1)%" } else { $"($v | math round -p 1)%" }
}

# Pull out the three-tier rows split by environment (local vs remote)
# and compute the analysis numbers we'll quote in prose.
def analysis-stats [pairs: list] {
  let local = ($pairs | where {|r| not ($r.op | str contains "(remote)") })
  let remote = ($pairs | where {|r| $r.op | str contains "(remote)" })

  let bind_local = if ($local | is-empty) { null } else {
    ($local | get js_vs_server_pct | math median)
  }
  let bind_remote = if ($remote | is-empty) { null } else {
    ($remote | get js_vs_server_pct | math median)
  }
  let typed_local = if ($local | is-empty) { null } else {
    ($local | get rust_vs_js_pct | math median)
  }
  let typed_remote = if ($remote | is-empty) { null } else {
    ($remote | get rust_vs_js_pct | math median)
  }
  let server_local_rps = if ($local | is-empty) { null } else {
    ($local | get server_rps | math median)
  }
  let server_remote_rps = if ($remote | is-empty) { null } else {
    ($remote | get server_rps | math median)
  }
  {
    have_local: (not ($local | is-empty))
    have_remote: (not ($remote | is-empty))
    bind_local: $bind_local
    bind_remote: $bind_remote
    typed_local: $typed_local
    typed_remote: $typed_remote
    server_local_rps: $server_local_rps
    server_remote_rps: $server_remote_rps
  }
}

def render-analysis [pairs: list] {
  let s = (analysis-stats $pairs)
  mut lines = []
  $lines = ($lines | append "")
  $lines = ($lines | append "## Analysis")
  $lines = ($lines | append "")
  $lines = ($lines | append "Headline takeaways from the latest data above. Numbers update automatically when `cf:fs:bench:report` runs.")
  $lines = ($lines | append "")

  if $s.have_local and $s.have_remote {
    let ratio = (($s.server_local_rps / $s.server_remote_rps) | math round -p 1)
    $lines = ($lines | append $"**Dev vs. real edge.** Local `wrangler dev` reports server-direct throughput around ($s.server_local_rps | math round -p 0) rps; the deployed Worker at the same op manages ($s.server_remote_rps | math round -p 0) rps -- the dev numbers are ~($ratio)x higher because wrangler dev runs on your laptop without real network RTT. Quote remote rows when comparing to production; quote local rows only for relative deltas and regression-spotting.")
    $lines = ($lines | append "")
  } else if $s.have_local {
    $lines = ($lines | append "**No remote data yet** -- run `mise run cf:fs:bench:remote` against deployed Workers to compare dev vs. real-edge numbers. Local rows alone give relative deltas, not absolute production performance.")
    $lines = ($lines | append "")
  } else {
    $lines = ($lines | append "**No local data yet** -- run `mise run cf:fs:bench:all` to see local-dev relative deltas alongside remote numbers.")
    $lines = ($lines | append "")
  }

  $lines = ($lines | append "**Binding + RPC overhead.** `demo-js` vs `server` measures the cost of going through a service-binding RPC instead of hitting the server's HTTP route directly. A small or negative number means the binding hop is essentially free.")
  if $s.have_local {
    $lines = ($lines | append $"- Local-dev median: **(fmt-pct $s.bind_local)**.")
  }
  if $s.have_remote {
    $lines = ($lines | append $"- Real edge median: **(fmt-pct $s.bind_remote)**.")
  }
  if $s.have_local and $s.have_remote {
    $lines = ($lines | append "")
    $lines = ($lines | append "If the local and remote numbers disagree wildly (e.g. local shows -60% on list, remote shows -12%), trust the remote -- local-dev's binding implementation is single-process workerd-on-Node, not what production runs.")
  }
  $lines = ($lines | append "")

  $lines = ($lines | append "**Typed Rust client cost.** `demo-rust` vs `demo-js` isolates the `cloudflare-shell-rpc-client` wrapper (hand-written wasm-bindgen extern + `serde-wasm-bindgen` round-trip). Positive = Rust faster, negative = the wrapper is overhead.")
  if $s.have_local {
    $lines = ($lines | append $"- Local-dev median: **(fmt-pct $s.typed_local)**.")
  }
  if $s.have_remote {
    $lines = ($lines | append $"- Real edge median: **(fmt-pct $s.typed_remote)**.")
  }
  $lines = ($lines | append "")
  $lines = ($lines | append "The wrapper is essentially free for primitives. On big-response ops (`stat` / `list` with non-trivial JSON), the JS-side parses native; the Rust side does an extra `serde_wasm_bindgen::from_value`. Worth re-measuring if it ever pushes past ~30%.")
  $lines = ($lines | append "")

  $lines = ($lines | append "**What single-run numbers do NOT tell you.** Each bench row is one ~3-10s oha sample. Same-op runs vary 5-20% across runs (wrangler dev sometimes more); single-row outliers (especially `(remote)` rows during peak edge load) shouldn't be over-fit. The **rolling averages** section below smooths this; the **history** section is the raw trail. For a defensible production claim, run `cf:fs:bench:remote` several times across different times of day and quote the median.")
  $lines = ($lines | append "")

  $lines = ($lines | append "**When numbers are 0 / NaN.** The bench parser pulls `rps` / `avg` / `p99` from oha's text output. If `ok_count` is 0 but `rps` is non-zero, oha got responses but they weren't HTTP 2xx -- usually wrangler dev cracking under sustained load, or a deployed Worker hitting a rate-cap. Treat those rows as bench failures, not slow performance.")

  $lines | str join "\n"
}

# Three-way comparison: for each operation we have a `server X` row
# (no binding hop), a `demo-js X` row (RPC through binding), and a
# `demo-rust X` row (RPC through binding + typed Rust client wrapper).
# Lines up by trimming the tier prefix.
def by-tier [rows: list] {
  let latest = ($rows
    | group-by label
    | items {|k v| {label: $k, latest: ($v | sort-by when | last)}})

  # Use server rows as the spine; for each, find matching demo-js / demo-rust.
  let server_rows = ($latest | where {|r| $r.label | str starts-with "server " })

  $server_rows | each {|s|
      let op = ($s.label | str substring 7..)   # strip "server "
      let js = ($latest | where label == $"demo-js ($op)" | get 0? | get latest? )
      let rs = ($latest | where label == $"demo-rust ($op)" | get 0? | get latest? )
      if $js == null or $rs == null {
        null
      } else {
        let s_rps = $s.latest.requests_per_sec
        let js_rps = $js.requests_per_sec
        let rs_rps = $rs.requests_per_sec
        let bind_cost_pct = if $s_rps > 0 {
          ((($js_rps - $s_rps) / $s_rps) * 100 | math round -p 1)
        } else { 0.0 }
        let typed_cost_pct = if $js_rps > 0 {
          ((($rs_rps - $js_rps) / $js_rps) * 100 | math round -p 1)
        } else { 0.0 }
        {
          op: $op
          server_rps: $s_rps
          js_rps: $js_rps
          rust_rps: $rs_rps
          js_vs_server_pct: $bind_cost_pct      # cost of demo + RPC binding hop
          rust_vs_js_pct: $typed_cost_pct       # cost of typed Rust wrapper
        }
      }
    }
    | where {|r| $r != null }
    | sort-by op
}

def humanize [n: int]: nothing -> string {
  if $n >= 1_048_576 {
    let mib = ($n / 1_048_576.0 | math round -p 2)
    $"($mib) MB"
  } else if $n >= 1024 {
    let kib = ($n / 1024.0 | math round -p 1)
    $"($kib) KB"
  } else {
    $"($n) B"
  }
}

# Pick out the latest size row per worker from sizes.nuon and shape it
# for the markdown table. Returns [] if no sizes data has been captured
# yet (sizes.nuon missing or empty).
def latest-sizes [script_dir: string] {
  let p = ($script_dir | path join "sizes.nuon")
  if not ($p | path exists) { return [] }
  let rows = (open $p)
  if ($rows | is-empty) { return [] }
  $rows
    | group-by worker
    | items {|k v| $v | sort-by when | last }
    | sort-by worker
    | each {|r| {
        worker: $r.worker
        raw_total: (humanize $r.raw_total)
        gz_total: (humanize $r.gz_total)
        "vs 1MB (self)": $"($r.budget_self_pct)%"
        "vs 3MB (free)": $"($r.budget_free_pct)%"
        "vs 10MB (paid)": $"($r.budget_paid_pct)%"
        captured: $r.when
      }}
}

# Classify a results.nuon row as local-dev (wrangler dev hitting
# 127.0.0.1 / localhost) vs remote (deployed Worker on the real CF
# edge). Used to split results into REPORT.local.md vs REPORT.remote.md.
def is-local-row [row]: nothing -> bool {
  ($row.target | str contains "127.0.0.1") or ($row.target | str contains "localhost")
}

# Render a single scope's report markdown. `scope` is "local" or
# "remote" and drives the prose differences (dev banner on local, no
# banner on remote, etc.). Deployment sizes are scope-agnostic so they
# render into both files identically.
def render-scope [rows: list, scope: string, sizes_rows: list]: nothing -> string {
  if ($rows | is-empty) {
    return $"# cloudflare-shell-rpc/bench -- ($scope) report\n\n_(no ($scope) results yet -- run `mise run cf:fs:bench:($scope)` to populate)_\n"
  }
  let total = ($rows | length)
  let latest = ($rows
    | group-by label
    | items {|k v| $v | sort-by when | last }
    | sort-by requests_per_sec --reverse
    | select label requests_per_sec avg_ms p50_ms p99_ms ok_count err_count when)

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

  let pairs = (by-tier $rows)

  let history = ($rows
    | sort-by when --reverse
    | select when label requests_per_sec p50_ms p99_ms ok_count err_count
    | first 20)

  let when_latest = ($rows | sort-by when | last | get when)
  let title_suffix = if $scope == "local" { "(local)" } else { "(remote)" }
  let banner = if $scope == "local" {
    "
> ## ⚠️ DEV NUMBERS, NOT PRODUCTION
>
> These rows come from `wrangler dev`: unoptimised wasm, debug profile,
> hosted in workerd-on-Node. They are useful for spotting **regressions**
> and JS-vs-Rust **relative** differences but are **not** representative
> of production rps / latency. The deployed-Worker numbers live in
> [`REPORT.remote.md`](./REPORT.remote.md).
"
  } else {
    "
> _Production rows -- requests against deployed Workers at_
> `*.gedw99.workers.dev`. _For the dev-mode counterpart see_
> [`REPORT.local.md`](./REPORT.local.md).
"
  }
  let regen_cmd = if $scope == "local" { "mise run cf:fs:bench:local" } else { "mise run cf:fs:bench:remote" }
  let regen_note = $"Regenerate this report with `($regen_cmd) && mise run cf:fs:bench:report`."

  let header = $"# cloudflare-shell-rpc/bench -- ($title_suffix)
($banner)
Auto-generated from `crates/cloudflare-shell-rpc/bench/results.nuon`. ($regen_note)

- Latest run captured: `($when_latest)`
- Total runs recorded: ($total)

## Latest snapshot per label

"
  let s_pairs = "
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

"
  let s_analysis = (render-analysis $pairs)
  let s_sizes_header = "
## Deployment sizes

Per-Worker bundle size at last measurement. `gz_total` is what CF charges
against the script-size limit; raw is the on-disk bundle before
compression. Budgets:

- **1 MB (self)** -- self-imposed for this subsystem (each Worker is meant
  to compose with others via service binding -- staying small is the point).
- **3 MB (free)** -- CF Workers free-plan ceiling.
- **10 MB (paid)** -- CF Workers paid-plan ceiling.

Regenerate via `mise run cf:fs:bench:sizes`.

"
  let s_sizes_body = if ($sizes_rows | is-empty) {
    "_(no sizes captured yet -- run `mise run cf:fs:bench:sizes` to populate)_\n"
  } else {
    md-table $sizes_rows
  }
  let s2 = "
## Rolling averages

How each label performs across every run we've captured.

"
  let s3 = "
## Recent history -- last 20 rows

"
  let footer = "
---

Notes:
- The JS demo speaks JSON to the binding directly. The Rust demo goes
  through the typed `cloudflare-shell-rpc-client` wrapper (serde-wasm-bindgen
  encode/decode). The `rust_vs_js_pct` column isolates that overhead.
- Cold start adds latency to the first request after a fresh DO isolate.
- The seed step runs once before each bench so GET /fs paths always read
  a file of the configured size.
"
  $header + (md-table $latest) + $s_pairs + (md-table $pairs) + $s_analysis + "\n" + $s_sizes_header + $s_sizes_body + $s2 + (md-table $rolling) + $s3 + (md-table $history) + $footer
}

def main [
  --save (-s)
] {
  let script_dir = ($env.FILE_PWD? | default ".")
  let data_path = $"($script_dir)/results.nuon"

  if not ($data_path | path exists) {
    print $"✗ no results yet -- run `mise run cf:fs:bench:local` first"
    exit 1
  }

  let rows = open $data_path
  let sizes_rows = (latest-sizes $script_dir)
  let local_rows = ($rows | where {|r| is-local-row $r })
  let remote_rows = ($rows | where {|r| not (is-local-row $r) })

  let local_md = (render-scope $local_rows "local" $sizes_rows)
  let remote_md = (render-scope $remote_rows "remote" $sizes_rows)

  if $save {
    let local_out = ($script_dir | path join "REPORT.local.md")
    let remote_out = ($script_dir | path join "REPORT.remote.md")
    $local_md | save -f $local_out
    $remote_md | save -f $remote_out
    print $"✓ wrote ($local_out)"
    print $"✓ wrote ($remote_out)"
  } else {
    print "=== REPORT.local.md ==="
    print $local_md
    print ""
    print "=== REPORT.remote.md ==="
    print $remote_md
  }
}
