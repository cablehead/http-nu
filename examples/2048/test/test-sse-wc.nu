use std/assert

# /sse-wc integration check for examples/2048. Run via
# `http-nu eval --services --store <path>` so actors run alongside.
#
# Drives the SSE handler end-to-end with the snapshot-actor turning
# fed `.move` frames into snapshots, and asserts the per-event SSE
# contract:
#
#   threshold flush     -> a patch with boardState (the latest state)
#   no-op move          -> a boardState patch with unchanged state
#                          (the actor emits an ephemeral snapshot
#                          carrying the move's req_id)
#   state-changing move -> a boardState patch with the new state
#                          (the durable snapshot the actor appends)
#   undo                -> a boardState patch reverting to the parent
#
# Determinism trick: after the actor writes the root snapshot, we
# append a synthetic snapshot whose state has two value-2 tiles at
# (0,0) and (3,0). On that head, `h` is a guaranteed no-op (both
# already at c=0); `j` merges them (state-changing); undo walks
# back to the synthetic. Avoids guessing the actor's random spawn.
#
# The consumer takes exactly N `data:` lines (N = length of the
# expectation list); `first N` is what cleanly drops the upstream
# `.cat --follow` (same shutdown path test-sse.nu's hang-guard uses).
# `generate { ... {} }` looked promising for state + termination but
# returning empty doesn't propagate the drop signal -- the script
# hangs. We resolve the per-line expectation by index instead.

const SCRIPT_DIR = path self | path dirname

# --- setup -----------------------------------------------------------------

open ($SCRIPT_DIR | path join ".." "tfe" "game.nu")           | .append xs.module.game                 --ttl last:1
open ($SCRIPT_DIR | path join ".." "tfe" "snapshot-actor.nu") | .append xs.actor.snapshot-actor.create --ttl last:1
sleep 500ms

let g = (null | .append "player.test-uid.games")
sleep 800ms
let root = .last $"game.snapshot.($g.id)"
assert ($root != null) "harness: root snapshot exists"

# Synthetic head: two value-2 tiles, one at top-left, one at bottom-left.
let controlled = $root.meta.state
  | upsert tiles [
      {id: 100 r: 0 c: 0 value: 2 spawned: false merged: false}
      {id: 101 r: 3 c: 0 value: 2 spawned: false merged: false}
    ]
  | upsert ghosts []
  | upsert next_id 102
  | upsert score 0
  | upsert game_over false

null | .append $"game.snapshot.($g.id)" --meta {
  state: $controlled
  last_move_id: $g.id
  prev: $root.id
  intent: "setup"
  player_id: "test-uid"
  req_id: "setup"
  score: 0
  max_tile: 2
  moves: 0
  game_over: false
}
sleep 200ms

# --- the contract we're pinning --------------------------------------------

let expectations = [
  { kind: "initial"                          }   # threshold flush of the synthetic
  { kind: "boardState" req_id: "req-noop"    }   # no-op 'h' -- ephemeral snapshot, unchanged state
  { kind: "boardState" req_id: "req-real"    }   # state-change 'j' -- durable snapshot
  { kind: "boardState" req_id: "req-undo"    }   # undo -- durable snapshot, reverts to synthetic
]

# --- background feeder -----------------------------------------------------
# 600ms head start so the consumer is past the threshold flush; 300ms gap
# so the actor processes each move before the next one is appended.

let feeder = job spawn {
  sleep 600ms
  null | .append $"game.move.($g.id)" --meta { user_id: "test-uid" session_id: "s" req_id: "req-noop" intent: "h" }
  sleep 300ms
  null | .append $"game.move.($g.id)" --meta { user_id: "test-uid" session_id: "s" req_id: "req-real" intent: "j" }
  sleep 300ms
  null | .append $"game.move.($g.id)" --meta { user_id: "test-uid" session_id: "s" req_id: "req-undo" kind: "undo" }
}

# --- consumer --------------------------------------------------------------

let handler = source ($SCRIPT_DIR | path join ".." "serve.nu")
let sse_req = {
  method: "GET"
  uri: $"/sse-wc/($g.id)"
  path: $"/sse-wc/($g.id)"
  headers: {}
  query: {}
  mount_prefix: ""
}

let data_lines = (
  do $handler $sse_req
  | lines
  | where ($it | str starts-with "data:")
  | first ($expectations | length)
)

$data_lines | enumerate | each {|p|
  let signals = $p.item | str replace -r '^data:\s*signals\s*' '' | from json
  let want = $expectations | get $p.index
  match $want.kind {
    "initial" => {
      assert ("boardState" in $signals) $"step ($p.index) initial: has boardState"
    }
    "boardState" => {
      assert ("boardState" in $signals)                           $"step ($p.index) boardState: has boardState"
      assert (($signals.lastReqId? | default "") == $want.req_id) $"step ($p.index) boardState: lastReqId == ($want.req_id)"
    }
  }
} | ignore

# The feeder finishes naturally once its last `.append` returns -- the
# consumer can't have observed the undo events without it. No explicit
# join (Nushell has no `job wait`); the job is reaped on script exit.

print "examples/2048/test/test-sse-wc.nu: all assertions passed"
