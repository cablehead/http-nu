use std/assert

# SSE-pipeline regression tests for examples/2048. Requires --store
# (pings the in-process xs); the wrapper in `check.sh` mints a temp
# store and runs this script under a `timeout` so a hang (e.g. the
# `let stream = .cat --follow ...` footgun) becomes a non-zero exit
# instead of a CI lockup.

const script_dir = path self | path dirname
use ($script_dir | path join ".." "tfe" "sse.nu") *
use http-nu/datastar *

# -- T1: interleave argument shape ------------------------------------------
#
# `interleave` takes CLOSURES (`{|| ... }`), not stream values. Passing
# a stream value errors at runtime when the engine tries to invoke it
# as a closure -- but inside an SSE handler that error sits behind any
# upstream stream-collection (see T3) and shows up as a hang. Guard the
# correct shape and the wrong shape both, so future refactors can't
# silently swap them.

let t1a = [{a: 1}] | interleave { [{b: 2}] } | first 2
assert (($t1a | length) == 2) "T1a: interleave with closure produces items"

# The live regression was `interleave (presence-stream)` -- a function
# call whose return type the parser can't see (no annotation), so the
# literal-form parse error doesn't apply. Mirror that shape: declare
# the helper WITHOUT an output type so the parser can't reject the
# call statically; the bad invocation must trip at runtime, where the
# try/catch can observe it.
def t1b-make-stream [] { [{b: 2}] }
let t1b_errored = try {
  [{a: 1}] | interleave (t1b-make-stream) | first 1
  false
} catch { true }
assert $t1b_errored "T1b: interleave (value) -- not (closure) -- must error at runtime"

# -- T2: presence-stream emits a Datastar signal patch on the head ----------
#
# Seed `_presence.summary` so presence-stream has something to project
# on connect. The output of `presence-stream` is a single SSE event
# record per summary change; we assert the shape, not the content.

null | .append "_presence.summary" --ttl last:1 --meta {
  totalTabs: 3
  activeGames: 0
  byScope: {splash: 3}
  byGame: {}
}
let t2 = presence-stream | first
assert (($t2.event? | default "") == "datastar-patch-signals") "T2: presence-stream emits a Datastar signal patch"

# -- T2b: xs.threshold marker is filtered out before .meta access -----------
#
# `.cat --last N --follow` injects a synthetic `xs.threshold` frame
# after the historical replay (xs/api.rs:566). That frame has no
# `meta` column. Presence-stream's projection accessed `$f.meta`
# without a topic guard, so any reader connecting against an empty
# summary topic crashed:
#   Error: nu::shell::column_not_found
#   x Cannot find column 'meta'
# Mirror the projection inline against a fixture that contains both a
# threshold-shaped frame and a real summary, and assert only the
# summary survives the filter.

let t2b_mixed = [
  {topic: "xs.threshold", id: "thresh-0"}                                   # no meta column
  {topic: "_presence.summary", id: "sum-0", meta: {totalTabs: 7}}
]
let t2b_projected = $t2b_mixed
  | where ($it.topic? | default "") == "_presence.summary"
  | each {|f| {presence: ($f.meta | default {})} | to datastar-patch-signals}
assert (($t2b_projected | length) == 1) "T2b: xs.threshold marker is filtered out before .meta access"
assert ((($t2b_projected | first | get event?) | default "") == "datastar-patch-signals") "T2b: surviving summary projects to a signal patch"

# -- T3: /sse-wc route closure doesn't hang ---------------------------------
#
# This is the test that would have caught the live regression. The
# buggy shape was:
#
#   let board_stream = .cat --follow -T ... --from $game_id
#     | frames-to-states | ... | html-to-patches
#   $board_stream | interleave { presence-stream } | to sse
#
# `let` collects the pipeline before binding -- `.cat --follow` is
# infinite, so the let never returns and the closure hangs before any
# headers are sent. With this test under a wrapper `timeout 15`, the
# hang becomes a test failure rather than a CI lockup.
#
# The seed pings (a games_topic frame + a summary frame already from
# T2 above) give the interleaved pipeline something to emit, so on the
# fixed shape `first 1` resolves quickly.

let games_frame = null | .append "player.test-uid.games" --meta {}
let game_id = $games_frame.id
let handler = source ($script_dir | path join ".." "serve.nu")
let req = {
  method: "GET"
  uri: $"/sse-wc/($game_id)"
  path: $"/sse-wc/($game_id)"
  headers: {}
  query: {}
  mount_prefix: ""
}
# `to sse` yields a streaming string, not a list; take the first non-
# empty line. SSE chunks always start with `event: ...` or `data: ...`,
# so a non-empty first line is sufficient proof the handler emitted.
let t3 = do $handler $req | lines | first
assert (($t3 | str length) > 0) "T3: /sse-wc route emits at least one SSE chunk"

print "examples/2048/test/test-sse.nu: all assertions passed"
