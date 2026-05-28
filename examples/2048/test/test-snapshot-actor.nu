use std/assert

# Snapshot-actor integration check for examples/2048. Run via
# `http-nu eval --services --store <path>` so the actor dispatcher
# spawns alongside the eval (without --services there is no
# dispatcher and an appended frame produces nothing).
#
# Asserts the snapshot-actor's I/O contract end-to-end across the
# core game flow:
#   1. new game   -- a games frame yields the root snapshot
#   2. real move  -- a state-changing move yields one new snapshot
#                    with the right state and req_id
#   3. no-op move -- a move into a wall yields no new snapshot
#   4. undo       -- yields a snapshot whose state matches the
#                    parent (walks `prev`)

const SCRIPT_DIR = path self | path dirname

# Register what serve.nu registers at startup: the game.nu module the
# actor `use`s, and the snapshot-actor itself. The dispatcher picks up
# the .register frame and spawns the actor.
open ($SCRIPT_DIR | path join ".." "tfe" "game.nu")           | .append game.nu                 --ttl last:1
open ($SCRIPT_DIR | path join ".." "tfe" "snapshot-actor.nu") | .append snapshot-actor.register --ttl last:1
sleep 500ms

# /new appends a `player.<uid>.games` frame. The actor responds with the
# root snapshot for that game (game_id = the games frame's own id).
let g = (null | .append "player.test-uid.games")
sleep 800ms
let root = .last $"game.snapshot.($g.id)"
assert ($root != null) "new game: root snapshot exists"
assert ($root.meta.last_move_id == $g.id) "new game: root snapshot's last_move_id == game_id"

# --- 2. real move ----------------------------------------------------------
# Pick a direction guaranteed to change state on the 2-tile root board: if
# any tile is below the top row, `k` (up) slides; otherwise `j` (down) does.
let tiles = $root.meta.state.tiles
let intent = if ($tiles | any {|t| $t.r > 0 }) { "k" } else { "j" }
let move = (null | .append $"game.move.($g.id)" --meta {
  user_id: "test-uid"
  session_id: "s"
  req_id: "real-move"
  intent: $intent
})
sleep 500ms
let after_real = .last $"game.snapshot.($g.id)"
assert ($after_real.id != $root.id) "real move: a new snapshot was written"
assert ($after_real.meta.last_move_id == $move.id) "real move: snapshot's last_move_id == move frame id"
assert ($after_real.meta.req_id == "real-move") "real move: snapshot carries the move's req_id"
assert ($after_real.meta.prev == $root.id) "real move: snapshot's prev == root snapshot id"
assert ($after_real.meta.intent == $intent) "real move: snapshot's intent == the move's intent"

# --- 3. no-op move ---------------------------------------------------------
# Feed the same direction repeatedly until one feed produces no new snapshot
# -- that's the no-op. With a bounded board (max 16 tiles) and a spawn per
# state change, repeated same-direction moves saturate within a small budget.
mut saw_noop = false
mut count = (.cat | where topic == $"game.snapshot.($g.id)" | length)
for i in 0..30 {
  null | .append $"game.move.($g.id)" --meta {
    user_id: "test-uid"
    session_id: "s"
    req_id: $"noop-($i)"
    intent: $intent
  }
  sleep 350ms
  let now = (.cat | where topic == $"game.snapshot.($g.id)" | length)
  if $now == $count {
    $saw_noop = true
    break
  }
  $count = $now
}
assert $saw_noop "no-op move: a repeated same-direction move eventually produces no new snapshot"

# --- 4. undo ---------------------------------------------------------------
# Undo walks back via `meta.prev`. The new snapshot's tiles should match the
# parent's tiles (the actor clears spawned/merged flags on the way back).
let head_before_undo = .last $"game.snapshot.($g.id)"
let parent = .get $head_before_undo.meta.prev
null | .append $"game.move.($g.id)" --meta {
  user_id: "test-uid"
  session_id: "s"
  req_id: "undo-1"
  kind: "undo"
}
sleep 500ms
let after_undo = .last $"game.snapshot.($g.id)"
assert ($after_undo.id != $head_before_undo.id) "undo: a new snapshot was written"
assert ($after_undo.meta.intent == "undo") "undo: snapshot's intent == \"undo\""
let undo_tiles  = $after_undo.meta.state.tiles | each {|t| {r: $t.r, c: $t.c, value: $t.value}} | sort-by r c
let parent_tiles = $parent.meta.state.tiles  | each {|t| {r: $t.r, c: $t.c, value: $t.value}} | sort-by r c
assert ($undo_tiles == $parent_tiles) "undo: snapshot's tiles match the parent's"

print "examples/2048/test/test-snapshot-actor.nu: all assertions passed"
