#!/usr/bin/env nu

# Bench matrix orchestrator. Calls `bench/run.nu` once per (label, url,
# path) row so REPORT.local.md / REPORT.remote.md shows the full
# JS-vs-Rust grid for the same duration / connection count.
#
# Doesn't spin up Workers -- assume `mise run cf:fs:up` is running.
#
# Use from mise:
#   mise run cf:fs:bench:local    # localhost ports
#   mise run cf:fs:bench:remote   # deployed Workers
#
# Direct:
#   nu crates/cloudflare-shell-rpc/bench/matrix.nu \
#     --url-js  http://127.0.0.1:8789 \
#     --url-rust http://127.0.0.1:8790 \
#     --duration 10s --connections 50

def main [
  --url-js: string = "http://127.0.0.1:8789"
  --url-rust: string = "http://127.0.0.1:8790"
  --url-server: string = "http://127.0.0.1:8788"  # raw server (no demo, no binding)
  --duration: string = "10s"
  --connections: int = 50
  --seed-size: int = 1024
  --tag: string = ""           # Suffix appended to every label (e.g. " (remote)")
  --auth-server (-a): string = "" # Bearer token sent only to the server-direct rows (skip if SHELL_FS_TOKEN unset on the server)
] {
  let script_dir = ($env.FILE_PWD? | default ".")
  let runner = ($script_dir | path join "run.nu")
  let read_label = $"read ($seed_size)B"

  # Each demo gets its own namespace so they don't warm each other's
  # DO cache. VALID_NAMESPACE forbids hyphens -- use underscore form.
  let ns_js = "benchjs"
  let ns_rust = "benchrust"

  # Three target tiers (each on its own namespace so they don't warm
  # each other's DO cache):
  #   server     -- direct HTTP at the server worker; no binding hop, no demo.
  #   demo-js    -- HTTP at demo-js, which dispatches via service-binding RPC.
  #   demo-rust  -- HTTP at demo-rust (typed Rust client wrapper) via RPC.
  #
  # Subtracting `demo-* vs server` reveals the binding+RPC overhead.
  # Subtracting `demo-rust vs demo-js` reveals the typed-client cost.
  let ns_server = "benchserver"
  let matrix = [
    {label: "server banner (no binding)",      url: $url_server, path: "/", path_for_seed: ""}
    {label: "demo-js banner",                  url: $url_js,     path: "/", path_for_seed: ""}
    {label: "demo-rust banner",                url: $url_rust,   path: "/", path_for_seed: ""}

    {label: $"server ($read_label)",           url: $url_server, path: $"/fs/($ns_server)/payload.bin", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: $"demo-js ($read_label)",          url: $url_js,     path: $"/fs/($ns_js)/payload.bin",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: $"demo-rust ($read_label)",        url: $url_rust,   path: $"/fs/($ns_rust)/payload.bin",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}

    {label: "server stat",                     url: $url_server, path: $"/stat/($ns_server)/payload.bin", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: "demo-js stat",                    url: $url_js,     path: $"/stat/($ns_js)/payload.bin",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: "demo-rust stat",                  url: $url_rust,   path: $"/stat/($ns_rust)/payload.bin",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}

    {label: "server list",                     url: $url_server, path: $"/list/($ns_server)/", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: "demo-js list",                    url: $url_js,     path: $"/list/($ns_js)/",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: "demo-rust list",                  url: $url_rust,   path: $"/list/($ns_rust)/",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}

    # `exists` -- cheap presence probe. One row from index lookup; the
    # baseline against which fancier reads (glob, info) can be compared.
    {label: "server exists",                   url: $url_server, path: $"/exists/($ns_server)/payload.bin", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: "demo-js exists",                  url: $url_js,     path: $"/exists/($ns_js)/payload.bin",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: "demo-rust exists",                url: $url_rust,   path: $"/exists/($ns_rust)/payload.bin",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}

    # `glob` -- LIKE-pattern scan over the namespace's index. With one
    # seeded file the pattern matches one row, so this measures the
    # scan + serialize floor (cf the full-table scan in `info`).
    {label: "server glob",                     url: $url_server, path: $"/glob/($ns_server)?pattern=/*.bin", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: "demo-js glob",                    url: $url_js,     path: $"/glob/($ns_js)?pattern=/*.bin",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: "demo-rust glob",                  url: $url_rust,   path: $"/glob/($ns_rust)?pattern=/*.bin",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}

    # `info` -- workspace aggregate. Single SUM(CASE...) over every
    # row, no path lookup. Tells you the cost ceiling of full-table
    # scans in this DO SQLite setup.
    {label: "server info",                     url: $url_server, path: $"/info/($ns_server)", path_for_seed: $"/fs/($ns_server)/payload.bin"}
    {label: "demo-js info",                    url: $url_js,     path: $"/info/($ns_js)",     path_for_seed: $"/fs/($ns_js)/payload.bin"}
    {label: "demo-rust info",                  url: $url_rust,   path: $"/info/($ns_rust)",   path_for_seed: $"/fs/($ns_rust)/payload.bin"}
  ]

  for row in $matrix {
    let label = $row.label + $tag
    # Only the `server *` rows hit the server's HTTP routes (which
    # check Authorization: Bearer when SHELL_FS_TOKEN is set). Demo
    # rows go through the demos' own routes -- the demos read their
    # own env.SHELL_FS_TOKEN and forward to the RPC binding.
    let row_auth = if ($row.label | str starts-with "server ") { $auth_server } else { "" }

    # `run.nu` only seeds when the path starts with /fs/; for stat /
    # list rows we want the seed to land at the payload path even
    # though we benchmark a different URL. Pass --path for the bench
    # and rely on a separate seed step.
    if $row.path_for_seed != "" and ($row.path_for_seed != $row.path) {
      # Pre-seed via run.nu's seed-only short-circuit isn't exposed
      # -- do an out-of-band PUT to materialise the file.
      let seed_url = $"($row.url)($row.path_for_seed)"
      let body = (1..$seed_size | each { 'x' } | str join)
      let auth_args = if ($row_auth | is-empty) { [] } else { ["-H" $"Authorization: Bearer ($row_auth)"] }
      curl -fsS -X PUT --data $body -H 'content-type: application/octet-stream' ...$auth_args $seed_url | ignore
    }
    ^nu $runner --url $row.url --path $row.path --duration $duration --connections $connections --seed-size $seed_size --label $label --auth $row_auth --save
    # wrangler dev gets flaky under sustained back-to-back load: the
    # last row in the matrix (typically demo-rust list) often returns
    # zero-latency / zero-count from oha because the worker stopped
    # responding mid-test. A 1s breather is empirically enough to keep
    # the dev process happy for ~10 rows. Production benches don't see
    # this -- it's a local-only stability hack.
    sleep 1sec
  }

  print $"✓ bench matrix done -- run `mise run cf:fs:bench:report` to render REPORT.local.md + REPORT.remote.md"
}
