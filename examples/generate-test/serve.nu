# generate-test: verify `generate` works on CF
#
# Purpose: prove or disprove that stock `nu-command::Generate` survives
# our wasm build without a shadow. PORT_STATUS.md (and the older
# CLOUDFLARE_STATUS survey) once claimed `generate` was missing on CF;
# source-level recon says otherwise (the `generators` module in
# nu-command/src/lib.rs:14 is NOT feature-gated, `Generate` is
# unconditionally registered in default_context.rs:470, the impl uses
# only `nu_engine::ClosureEval` + `nu_protocol::Closure` -- no OS deps).
#
# This handler exercises `generate` end-to-end so we can confirm with
# a single curl call against either target.
#
# Desktop:
#   mise run ex:generate-test   # if a mise task exists; otherwise:
#   cargo run -- 0.0.0.0:3001 examples/generate-test/serve.nu
#   curl http://127.0.0.1:3001/
#
# CF local:
#   CF_HANDLER_PATH=examples/generate-test/serve.nu mise run cf:dev
#   curl http://127.0.0.1:8787/alice/
#
# Expected output on both: `1,1,2,3,5,8,13,21,34,55` (first 10 Fibonacci)
#
# If this works on CF: confirms `generate` doesn't need a shadow.
#   -> Move the row out of "shadow targets" in PORT_STATUS.md.
# If this fails to parse on CF: we have an actual mismatch worth
# investigating before shadowing.

{|req|
  let fib = generate {|pair = [1 1]|
    let next = ($pair.0 + $pair.1)
    {out: $pair.0, next: [$pair.1, $next]}
  } | first 10 | str join ","
  $fib
}
