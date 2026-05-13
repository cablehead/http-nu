#!/usr/bin/env nu

# End-to-end smoke test for cloudflare-shell-rpc.
#
# Drives the target's curl-able HTTP surface (PUT / GET / DELETE / stat /
# list / mkdir). Works against:
#   * a demo Worker (:8789 JS, :8790 Rust) -- demo forwards into the
#     FS-RPC server over a service binding; auth (if any) is handled
#     by the demo reading env.SHELL_FS_TOKEN.
#   * the FS-RPC server directly (:8788) -- HTTP routes mounted in
#     the server's #[event(fetch)]. If the server has SHELL_FS_TOKEN
#     set, pass --auth=<token> on this command.
#
# Use from mise:
#   mise run cf:fs:smoke                # JS demo
#   mise run cf:fs:smoke:rust           # Rust demo
#   mise run cf:fs:smoke:server         # server direct (set SHELL_FS_TOKEN env if auth is on)
#   LIVE_BASE=https://...  mise run cf:fs:smoke
#
# Direct invocation:
#   nu crates/cloudflare-shell-rpc/smoke/run.nu --url http://127.0.0.1:8789
#   nu crates/cloudflare-shell-rpc/smoke/run.nu --url https://demo-js.example.workers.dev
#   nu crates/cloudflare-shell-rpc/smoke/run.nu --url https://shell-fs-rpc.example.workers.dev --auth abc123
#
# Exit codes: 0 ok, non-zero on any assertion failure.

def fail [msg: string] {
  error make {msg: $msg}
}

# Header list to pass to every http call. nu's `http` builtin takes
# `--headers [name value ...]` (flat alternating).
def auth_headers [auth: string] {
  if ($auth | is-empty) { [] } else { ["Authorization" $"Bearer ($auth)"] }
}

def expect-rejected [url: string, auth: string, ctx: string] {
  # Without `--allow-errors`, nu's `http get` raises on 4xx/5xx --
  # which is exactly the signal we want for "this should fail."
  let headers = (auth_headers $auth)
  let result = (try {
    if ($headers | is-empty) { http get $url } else { http get --headers $headers $url }
    "accepted"
  } catch {
    "rejected"
  })
  if $result == "accepted" {
    fail $"expected ($ctx) to be rejected, but request succeeded"
  }
}

def main [
  --url (-u): string = "http://127.0.0.1:8789"  # Base URL of the target
  --namespace (-n): string = "smoke"            # Workspace namespace
  --path (-p): string = "/hello.txt"            # File path under the namespace
  --body (-b): string = "hello from cf:fs:smoke" # Round-trip body
  --auth (-a): string = ""                      # Bearer token (if target has SHELL_FS_TOKEN set)
] {
  let auth_label = if ($auth | is-empty) { "(no-auth)" } else { "(with-auth)" }
  print $"→ smoke target: ($url)  namespace=($namespace)  path=($path) ($auth_label)"

  let fs_url    = $"($url)/fs/($namespace)($path)"
  let stat_url  = $"($url)/stat/($namespace)($path)"
  let list_url  = $"($url)/list/($namespace)/"
  let mkdir_url = $"($url)/mkdir/($namespace)/sub"
  let sub_url   = $"($url)/fs/($namespace)/sub"
  let bad_url   = $"($url)/list/bad-ns/"   # hyphen -> fails VALID_NAMESPACE
  let headers = (auth_headers $auth)

  # 1. PUT
  print "  PUT"
  if ($headers | is-empty) {
    http put --content-type 'text/plain' $fs_url $body | ignore
  } else {
    http put --content-type 'text/plain' --headers $headers $fs_url $body | ignore
  }

  # 2. GET (verify body round-trip).
  # The demo Workers serve /fs/<ns><path> with content-type
  # application/octet-stream (they don't carry through the stored
  # mime_type), so nu hands us a binary blob -- decode it before
  # comparing to the round-trip body.
  print "  GET"
  let got_bytes = (if ($headers | is-empty) { http get $fs_url } else { http get --headers $headers $fs_url })
  let got = if ($got_bytes | describe) == "binary" {
    $got_bytes | decode utf-8
  } else {
    $got_bytes
  }
  if $got != $body {
    fail $"body mismatch: expected ($body | to nuon), got ($got | to nuon)"
  }

  # 3. stat
  print "  stat"
  let s = (if ($headers | is-empty) { http get $stat_url } else { http get --headers $headers $stat_url })
  let kind = ($s.stat?.kind? | default "")
  if $kind != "file" {
    fail $"stat missing kind=file: ($s | to nuon)"
  }

  # 4. mkdir + list (verify both file and dir appear)
  print "  mkdir /sub"
  if ($headers | is-empty) {
    http post $mkdir_url "" | ignore
  } else {
    http post --headers $headers $mkdir_url "" | ignore
  }
  print "  list /"
  let list_body = (if ($headers | is-empty) { http get $list_url } else { http get --headers $headers $list_url })
  let entries = ($list_body | get entries)
  let names = ($entries | get name)
  if not ("hello.txt" in $names) {
    fail $"list missing hello.txt: ($entries | to nuon)"
  }
  if not ("sub" in $names) {
    fail $"list missing sub: ($entries | to nuon)"
  }

  # 5. rm + GET-404
  print "  rm"
  if ($headers | is-empty) {
    http delete $fs_url | ignore
  } else {
    http delete --headers $headers $fs_url | ignore
  }
  print "  GET 404"
  expect-rejected $fs_url $auth "GET after rm"

  # Cleanup the subdir we made (force=1 lets us delete non-empty if needed).
  if ($headers | is-empty) {
    http delete $"($sub_url)?recursive=1&force=1" | ignore
  } else {
    http delete --headers $headers $"($sub_url)?recursive=1&force=1" | ignore
  }

  # 6. bad-namespace rejection. Defends against SQL identifier injection:
  #    the namespace flows into `cf_workspace_<ns>` in raw SQL DDL/queries
  #    (SqlStorage can't parameterise table names), so Workspace::new
  #    enforces upstream's VALID_NAMESPACE regex. A hyphen is enough to
  #    trip it -- and we want the target to surface that as a 4xx/5xx,
  #    not silently 200.
  print "  bad-namespace rejection"
  expect-rejected $bad_url $auth "invalid namespace 'bad-ns'"

  print $"✓ smoke ok against ($url) ($auth_label)"
}
