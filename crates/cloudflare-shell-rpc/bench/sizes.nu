#!/usr/bin/env nu

# Measure deployed bundle size for each cloudflare-shell-rpc Worker.
#
# The benchmark suite tracks RPS / latency in `results.nuon`; this script
# is the size-axis companion, writing one row per Worker to `sizes.nuon`.
# Why size matters: each Worker has its own CF script-size limit (3 MB
# compressed on the free plan, 10 MB on paid). The FS-RPC subsystem is
# designed as small, composable Workers consumed via service binding --
# so we additionally hold ourselves to a 1 MB self-imposed budget per
# Worker.
#
# Source of truth: `wrangler deploy --dry-run --outdir=<tmp>` produces
# exactly the artefact set wrangler uploads. We measure each file raw
# and gzip-9 (close proxy for what CF charges against the limit) and
# total it. Source maps (`*.map`) are excluded -- CF doesn't count them.
#
# Usage:
#   nu crates/cloudflare-shell-rpc/bench/sizes.nu             # print
#   nu crates/cloudflare-shell-rpc/bench/sizes.nu --save      # write sizes.nuon
#
# Wrapper: `mise run cf:fs:bench:sizes` (provides wrangler + worker-build).

# Each Worker in the FS-RPC subsystem. `kind` controls the pre-bundle
# step: Rust workers run `worker-build --release` first; JS workers go
# straight to wrangler.
def workers [] {
  [
    {name: "server",    dir: "crates/cloudflare-shell-rpc/server",    kind: "rust"}
    {name: "demo-js",   dir: "crates/cloudflare-shell-rpc/demo-js",   kind: "js"}
    {name: "demo-rust", dir: "crates/cloudflare-shell-rpc/demo-rust", kind: "rust"}
  ]
}

# CF script-size ceilings (compressed bytes). Workers count gzip; we
# include 1 MB as a self-imposed budget for the FS-RPC subsystem since
# each Worker here is meant to compose alongside others via service
# binding -- staying small is the whole point.
const BUDGET_SELF: int =  1_048_576    # 1 MiB
const BUDGET_FREE: int =  3_145_728    # 3 MiB
const BUDGET_PAID: int = 10_485_760    # 10 MiB

# Gzip-9 a file and return the compressed byte count.
def gzip9-bytes [path: string]: nothing -> int {
  ^gzip -c -9 $path | bytes length
}

# List every file under outdir, skipping source maps (CF doesn't count
# `.map` against the script-size limit).
def walk-bundle [outdir: string] {
  glob ($"($outdir)/**/*")
    | where {|p| (($p | path type) == "file") and (not ($p | str ends-with ".map")) }
    | each {|p|
        let raw = (ls $p | get 0.size | into int)
        let gz = (gzip9-bytes $p)
        {file: ($p | str substring (($outdir | str length) + 1)..), raw: $raw, gz: $gz}
      }
}

# Render a byte count to a human-readable "1.2 MB" / "742 KB" / "183 B"
# string. Keeps the columnar width predictable.
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

def measure-worker [repo_root: string, w: record] {
  let dir_abs = ($repo_root | path join $w.dir)
  let outdir = (mktemp -d)
  print $"→ measuring ($w.name)  -- outdir=($outdir)"

  if $w.kind == "rust" {
    # Ensure release wasm exists. `worker-build --release` is idempotent
    # if up-to-date; cheap enough to always run.
    cd $dir_abs
    if $w.name == "server" {
      # server uses the custom WorkerEntrypoint shim (env-injection
      # workaround; see crates/cloudflare-shell-rpc/server/shim.js).
      $env.CUSTOM_SHIM = ($dir_abs | path join "shim.js")
    }
    ^worker-build --release out+err> /dev/null
  }

  cd $dir_abs
  # `wrangler deploy --dry-run` builds the bundle locally without
  # uploading. `--outdir` materialises it so we can walk + size it.
  ^wrangler deploy --dry-run --outdir $outdir out+err> /dev/null

  let files = (walk-bundle $outdir)
  let raw_total = ($files | get raw | math sum)
  let gz_total = ($files | get gz | math sum)

  # Cleanup tmp so repeated runs don't leak.
  rm -rf $outdir

  {
    worker: $w.name
    files: $files
    raw_total: $raw_total
    gz_total: $gz_total
    budget_self_pct: (($gz_total / $BUDGET_SELF) * 100 | math round -p 1)
    budget_free_pct: (($gz_total / $BUDGET_FREE) * 100 | math round -p 1)
    budget_paid_pct: (($gz_total / $BUDGET_PAID) * 100 | math round -p 1)
    when: (date now | format date "%Y-%m-%dT%H:%M:%S")
  }
}

def main [
  --save (-s)              # append to sizes.nuon
] {
  let script_dir = ($env.FILE_PWD? | default ".")
  let repo_root = ($script_dir | path join ".." ".." ".." | path expand)

  let rows = (workers | each {|w| measure-worker $repo_root $w })

  let summary = ($rows | each {|r| {
    worker: $r.worker
    raw_total: (humanize $r.raw_total)
    gz_total: (humanize $r.gz_total)
    "vs 1MB (self)": $"($r.budget_self_pct)%"
    "vs 3MB (free)": $"($r.budget_free_pct)%"
    "vs 10MB (paid)": $"($r.budget_paid_pct)%"
  }})

  print ""
  print "=== Deployed bundle sizes ==="
  $summary | table --expand

  if $save {
    let out = ($script_dir | path join "sizes.nuon")
    let prev = if ($out | path exists) { open $out } else { [] }
    let updated = ($prev | append $rows)
    $updated | to nuon | save -f $out
    let n = ($updated | length)
    print $"Saved to ($out) -- ($n) rows"
  }

  $rows
}
