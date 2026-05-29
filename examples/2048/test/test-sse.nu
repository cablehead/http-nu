use std/assert

# SSE-pipeline regression tests for examples/2048. Requires --store (it
# appends to the in-process xs). check.sh mints a temp store and runs
# this under `timeout`, so a hang -- e.g. the `let s = .cat --follow ...`
# footgun below -- becomes a non-zero exit instead of a CI lockup.

const script_dir = path self | path dirname
use ($script_dir | path join ".." "tfe" "sse.nu") *
use http-nu/datastar *

# --- interleave needs closures, not stream values --------------------------
# The SSE handlers build output as `interleave { stream-a } { stream-b }`.
# Passed a stream *value* instead of a `{|| ... }` closure, interleave
# errors at runtime -- and inside an SSE handler that error hides behind
# upstream stream collection (see "/sse-wc emits without hanging" below)
# and surfaces as a hang. Guard both the right and the wrong shape.

let closure_items = [{a: 1}] | interleave { [{b: 2}] } | first 2
assert (($closure_items | length) == 2) "interleave with a closure produces items"

# The live bug was `interleave (presence-stream)` -- a call with no output
# annotation, so the parser can't reject it statically; it has to trip at
# runtime. Mirror that: a helper with no declared output type, invoked the
# wrong way, caught by try.
def make-stream [] { [{b: 2}] }
let value_errored = try {
  [{a: 1}] | interleave (make-stream) | first 1
  false
} catch { true }
assert $value_errored "interleave given a stream value (not a closure) errors at runtime"

# --- presence-stream emits a Datastar patch per summary --------------------
# Seed _presence.summary so presence-stream has something to project on
# connect. Assert the output shape -- one signal-patch event per summary
# change -- not the content.

null | .append "_presence.summary" --ttl last:1 --meta {
  totalTabs: 3
  activeGames: 0
  byScope: {splash: 3}
  byGame: {}
}
let presence_patch = presence-stream | first
assert (($presence_patch.event? | default "") == "datastar-patch-signals") "presence-stream projects a summary into a Datastar signal patch"

# --- presence-stream skips the synthetic xs.threshold marker ---------------
# A --follow stream gets an xs.threshold frame at the history->live
# boundary (xs/api.rs:566); it has no `meta` column. The projection read
# `$f.meta` with no topic guard, so connecting against an empty summary
# topic crashed with "Cannot find column 'meta'". Mirror the projection
# against a fixture holding both the marker and a real summary; assert
# only the summary survives the filter.

let marker_and_summary = [
  {topic: "xs.threshold", id: "thresh-0"}                                   # no meta column
  {topic: "_presence.summary", id: "sum-0", meta: {totalTabs: 7}}
]
let projected = $marker_and_summary
  | where ($it.topic? | default "") == "_presence.summary"
  | each {|f| {presence: ($f.meta | default {})} | to datastar-patch-signals}
assert (($projected | length) == 1) "xs.threshold marker is filtered out before .meta access"
assert ((($projected | first | get event?) | default "") == "datastar-patch-signals") "the surviving summary projects to a signal patch"

# --- /sse-wc emits without hanging -----------------------------------------
# Catches the let-collects-an-infinite-stream hang. The buggy shape was:
#   let s = .cat --follow ... | frames-to-states | ... | to sse
# `let` collects the pipeline before binding, and `.cat --follow` never
# ends -- so the handler hangs before sending headers. Drive the real
# route closure and assert it emits a chunk; the wrapper `timeout 15`
# turns a hang into a failure. The games + summary frames seeded above
# give the interleaved pipeline something to emit, so `first` resolves
# quickly on the correct shape.

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
# `to sse` yields a streaming string, not a list; the first non-empty
# line (an `event:`/`data:` chunk) is proof the handler emitted.
let first_chunk = do $handler $req | lines | first
assert (($first_chunk | str length) > 0) "/sse-wc emits at least one SSE chunk (didn't hang)"

print "examples/2048/test/test-sse.nu: all assertions passed"
