#!/usr/bin/env nu

# Benchmark cloudflare-shell-rpc end-to-end -- service-binding RPC
# from a demo Worker (JS or Rust) through to the FS-RPC server's
# DurableObject + R2 Workspace.
#
# Doesn't spin up servers (use `mise run cf:fs:up` first); seeds a
# fixed file once before benching so GET paths have something to
# read. Result row is appended to results.nuon when --save is set.
#
#   # local (after `mise run cf:fs:up`)
#   nu crates/cloudflare-shell-rpc/bench/run.nu --url http://127.0.0.1:8789 --path /fs/bench/payload.bin --save --label "demo-js read"
#   nu crates/cloudflare-shell-rpc/bench/run.nu --url http://127.0.0.1:8790 --path /fs/bench/payload.bin --save --label "demo-rust read"
#
# mise wraps the common cases:
#   mise run cf:fs:bench:local   # benches both demos against a known matrix
#   mise run cf:fs:bench:remote  # same but live deploy
#   mise run cf:fs:bench:report  # render REPORT.local.md + REPORT.remote.md
#
# Numbers to interpret:
#  - Local: dev-mode wasm in workerd-on-Node; numbers reflect your
#    laptop + dev profile, not prod.
#  - Remote: real CF edge with the FS-RPC binding + DO + R2 cold path
#    on first request.
#  - JS-vs-Rust spread: the JS demo speaks JSON directly; the Rust
#    demo runs the typed cloudflare-shell-rpc-client wrapper.
#    Comparing same path between :8789 and :8790 isolates client
#    overhead.

def seed [url: string, namespace: string, path: string, size: int, auth: string] {
  let target_put = $"($url)/fs/($namespace)($path)"
  let body = (1..$size | each { 'x' } | str join)
  let auth_args = if ($auth | is-empty) { [] } else { ["-H" $"Authorization: Bearer ($auth)"] }
  let resp = (curl -s -o /dev/null -w '%{http_code}' -X PUT --data $body -H 'content-type: application/octet-stream' ...$auth_args $target_put | complete)
  let code = ($resp.stdout | into int)
  if $code < 200 or $code >= 300 {
    error make {msg: $"seed PUT failed: HTTP ($code) on ($target_put)"}
  }
}

def main [
  --url (-u): string = "http://127.0.0.1:8789" # Base URL of the demo Worker
  --path (-p): string = "/fs/bench/payload.bin" # Path on that Worker (must start with /fs/, /list/, /stat/, or /)
  --duration (-d): string = "10s" # oha -z duration
  --connections (-c): int = 50    # oha -c connections
  --seed-size: int = 1024         # bytes to PUT into the read target (only for /fs/<ns>/<path>)
  --no-seed                       # skip the seed step (caller is responsible)
  --save (-s)                     # append to results.nuon
  --label (-l): string = ""       # row label
  --auth (-a): string = ""        # Bearer token threaded to seed-PUT + oha -H (only needed for server-direct targets that have SHELL_FS_TOKEN set)
] {
  let script_dir = ($env.FILE_PWD? | default ".")

  # If we're benching a /fs/ read, seed the file first. /list /stat /
  # are read-only and don't need writes; skip the seed for them.
  if (not $no_seed) and ($path | str starts-with "/fs/") {
    let parts = ($path | split row "/" | where {|s| $s != "" })
    if ($parts | length) >= 3 {
      let ns = $parts.1
      let fs_path = "/" + ($parts | skip 2 | str join "/")
      print $"→ seed ($url)/fs/($ns)($fs_path) -- ($seed_size) bytes"
      seed $url $ns $fs_path $seed_size $auth
    }
  }

  let target = $"($url)($path)"
  print $"→ oha -z ($duration) -c ($connections) ($target)"
  let auth_oha = if ($auth | is-empty) { [] } else { ["-H" $"Authorization: Bearer ($auth)"] }
  let oha_out = (oha -z $duration -c $connections ...$auth_oha $target | complete).stdout

  let rps = ($oha_out | parse -r 'Requests/sec:\s+([\d.]+)' | get 0?.capture0? | default "0" | into float)
  let avg = ($oha_out | parse -r 'Average:\s+([\d.]+)\s+(ms|secs|s)' | get 0? | default {capture0: "0" capture1: "ms"})
  let avg_ms = if $avg.capture1 == "secs" or $avg.capture1 == "s" {
    ($avg.capture0 | into float) * 1000
  } else {
    $avg.capture0 | into float
  }
  let p50 = ($oha_out | parse -r '50.00% in\s+([\d.]+)\s+(ms|secs|s)' | get 0?.capture0? | default "0" | into float)
  let p99 = ($oha_out | parse -r '99.00% in\s+([\d.]+)\s+(ms|secs|s)' | get 0?.capture0? | default "0" | into float)
  let codes_2xx = ($oha_out | parse -r '\[200\]\s+(\d+)\s+responses' | get 0?.capture0? | default "0" | into int)
  let non2xx_list = ($oha_out | parse -r '\[(\d{3})\]\s+(\d+)\s+responses'
    | where capture0 != "200"
    | each {|r| $r.capture1 | into int })
  let codes_non2xx = if ($non2xx_list | is-empty) { 0 } else { $non2xx_list | math sum }

  let result = {
    label: ($label | if ($in | is-empty) { $target } else { $in })
    target: $target
    duration: $duration
    connections: $connections
    requests_per_sec: ($rps | math round -p 2)
    avg_ms: ($avg_ms | math round -p 2)
    p50_ms: ($p50 | math round -p 2)
    p99_ms: ($p99 | math round -p 2)
    ok_count: $codes_2xx
    err_count: $codes_non2xx
    when: (date now | format date "%Y-%m-%dT%H:%M:%S")
  }

  print ""
  print "=== Result ==="
  [$result] | select label requests_per_sec avg_ms p50_ms p99_ms ok_count err_count | table

  if $save {
    let path = $"($script_dir)/results.nuon"
    let prev = if ($path | path exists) { open $path } else { [] }
    let updated = ($prev | append $result)
    $updated | to nuon | save -f $path
    let n = ($updated | length)
    print $"Saved to ($path) -- ($n) rows"
  }

  $result
}
