#!/usr/bin/env nu

# Benchmark an http-nu Worker -- local wrangler dev OR live deploy.
#
# Doesn't spin up a server (unlike the other benchmarks in this dir);
# assumes one is already listening. mise handles the lifecycle:
#
#   # local
#   mise run cf:dev:hub          # terminal 1
#   mise run cf:bench:local      # terminal 2
#
#   # remote
#   mise run cf:bench:remote
#
# Direct: `nu benchmarks/bench-cf/run.nu --url <URL> --path <PATH>`.
#
# Notes on what numbers mean:
#  - Local: wrangler dev runs an unoptimised wasm in a Node-hosted
#    workerd; numbers reflect dev-mode + your laptop, NOT prod.
#  - Remote: real Cloudflare edge. Closer to a production read.
#  - First request to a fresh DO is slower (cold-start + workspace
#    snapshot preload). The default duration (10s) and connections
#    (50) include warmup; the rps is the steady-state average.

def main [
  --url (-u): string = "http://127.0.0.1:8787" # Base URL (no trailing slash)
  --path (-p): string = "/" # Path to hit
  --duration (-d): string = "10s" # oha -z duration
  --connections (-c): int = 50 # oha -c connections
  --save (-s) # Save results to results.nuon next to this script
  --label (-l): string = "" # Optional tag for the result row
] {
  let script_dir = ($env.FILE_PWD? | default ".")
  let target = $"($url)($path)"

  print $"→ oha -z ($duration) -c ($connections) ($target)"
  let oha_out = (oha -z $duration -c $connections $target | complete).stdout

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
