use std/assert

# Source serve.nu so its `def` commands (apply-move, slide-row-tiles,
# tiles-equal, impulses-to-states, etc.) become callable in this scope.
# The returned closure is the request handler -- not exercised here; the
# end-to-end route behavior is covered by tests-browser/2048.test.mjs.
const script_dir = path self | path dirname
let _handler = source ($script_dir | path join serve.nu)

# --- pure game logic --------------------------------------------------------

# initial-state seeds two random tiles, score zero, not game-over.
let s0 = initial-state
assert (($s0.tiles | length) == 2) "initial state has 2 tiles"
assert ($s0.score == 0) "initial score 0"
assert (not $s0.game_over) "initial not game over"
assert ($s0.next_id >= 3) "next_id advanced past two spawned tiles"

# slide-row-tiles collapses adjacent equal values and accumulates score.
let row = [
  {id: 1 r: 0 c: 0 value: 2}
  {id: 2 r: 0 c: 1 value: 2}
  {id: 3 r: 0 c: 2 value: 4}
  {id: 4 r: 0 c: 3 value: 4}
] | slide-row-tiles 0
assert (($row.tiles | length) == 2) "two pairs merge into two tiles"
assert ($row.score == 12) "score is 4 + 8 = 12"
assert (($row.tiles | get 0 | get c) == 0) "first merged tile at column 0"
assert (($row.tiles | get 1 | get c) == 1) "second merged tile at column 1"

# slide-row-tiles preserves leading tile id on merge.
let merge_ids = [
  {id: 10 r: 0 c: 0 value: 2}
  {id: 11 r: 0 c: 1 value: 2}
] | slide-row-tiles 0
assert ((($merge_ids.tiles | get 0).id) == 10) "merge keeps leading tile id"

# apply-move shifts everything left when called with 'h'.
let pre = {
  tiles: [{id: 1 r: 0 c: 3 value: 4}]
  next_id: 2 score: 0 game_over: false
}
let post = $pre | apply-move "h" 0 0
let moved = $post.tiles | where id == 1 | first
assert ($moved.c == 0) "single tile moves to column 0 on left"
assert (($post.tiles | length) == 2) "spawn-tile added a second tile after the move"

# apply-move is identity (same tiles) when the move doesn't change the board.
let locked = {
  tiles: [{id: 1 r: 0 c: 0 value: 2}]
  next_id: 2 score: 0 game_over: false
}
let noop = $locked | apply-move "h" 0 0
assert (tiles-equal $locked.tiles $noop.tiles) "no-op move returns same tile list"
assert ($locked.score == $noop.score) "no-op move keeps score"

# tiles-equal is order-independent (sort-by id).
let a = [{id: 1 r: 0 c: 0 value: 2} {id: 2 r: 1 c: 1 value: 4}]
let b = [{id: 2 r: 1 c: 1 value: 4} {id: 1 r: 0 c: 0 value: 2}]
assert (tiles-equal $a $b) "tiles-equal ignores order"
let c = [{id: 1 r: 0 c: 0 value: 2} {id: 2 r: 1 c: 1 value: 8}]
assert (not (tiles-equal $a $c)) "tiles-equal detects value diff"

# Each direction reuses slide-tiles-left via reflect/transpose.
# A diagonal of 2s slides into a single 8 on slam-l in three steps.
let diag = {
  tiles: [
    {id: 1 r: 0 c: 0 value: 2}
    {id: 2 r: 0 c: 1 value: 2}
    {id: 3 r: 0 c: 2 value: 2}
    {id: 4 r: 0 c: 3 value: 2}
  ]
  next_id: 5 score: 0 game_over: false
}
let after_l = $diag | apply-move "l" 0 0
let rightmost = $after_l.tiles | where r == 0 and c == 3 | first
assert ($rightmost.value == 4) "right-side merge yields 4 on slide right"

# --- impulses-to-states stack discipline ------------------------------------

# A known starting state so we can reason about the stack precisely.
let known = {
  tiles: [
    {id: 1 r: 0 c: 1 value: 2}
    {id: 2 r: 0 c: 2 value: 2}
  ]
  next_id: 3 score: 0 game_over: false
}
let init = {stack: [$known] mode: "game"}

# Helper: feed a list of frames through impulses-to-states and return the
# emitted records as a list. Each frame emits one or more records.
def drive [frames: list, initial: record]: nothing -> list {
  $frames | impulses-to-states $initial
}

# 1. Start frame replaces the stack entirely.
let new_init = initial-state
let r1 = drive [{topic: "game.t.move" meta: {kind: "start" state: $new_init}}] $init
assert ((($r1 | length) == 1)) "start frame emits one state"
assert (tiles-equal ($r1 | first | get state | get tiles) $new_init.tiles) "start state is the seeded state"

# 2. xs.threshold marker passes the current top of stack through.
let r2 = drive [{topic: "xs.threshold"}] $init
assert (($r2 | first | get threshold) == true) "threshold marker carries threshold=true"
assert (tiles-equal ($r2 | first | get state | get tiles) $known.tiles) "threshold emits current top"

# 3. A move that changes the board emits one state; a no-op move emits an
#    echo of the same state (changed=false). Stack discipline is verified
#    by the next test, which chains move -> undo and confirms restoration.
let r3 = drive [{topic: "game.t.move" meta: {intent: "h" spawn_idx: 0 spawn_value: 0}}] $init
let r3_state = $r3 | first | get state
let changed = not (tiles-equal $r3_state.tiles $known.tiles)
assert $changed "left move on known state changes the board"
assert (($r3 | first | get direction) == "h") "move record carries direction"

# 4. Undo after a real move restores the pre-move state.
let move_then_undo = [
  {topic: "game.t.move" meta: {intent: "h" spawn_idx: 0 spawn_value: 0}}
  {topic: "game.t.move" meta: {kind: "undo"}}
  {topic: "xs.threshold"}
]
let r4 = drive $move_then_undo $init
let r4_final = $r4 | where threshold == true | first
assert (tiles-equal $r4_final.state.tiles $known.tiles) "undo restores the prior state"

# 5. Undo at the bottom of the stack is a no-op echo (state unchanged).
let r5 = drive [{topic: "game.t.move" meta: {kind: "undo"}}] $init
assert (tiles-equal ($r5 | first | get state | get tiles) $known.tiles) "undo at bottom echoes current"

# 6. A no-op move (intent that doesn't change the board) does NOT push,
#    so a subsequent undo would have nothing to pop -- it would echo the
#    same state. Construct a board locked on the left edge and try left.
let edge = {
  tiles: [{id: 1 r: 0 c: 0 value: 2}]
  next_id: 2 score: 0 game_over: false
}
let init_edge = {stack: [$edge] mode: "game"}
let r6 = drive [
  {topic: "game.t.move" meta: {intent: "h" spawn_idx: 0 spawn_value: 0}}
  {topic: "game.t.move" meta: {kind: "undo"}}
  {topic: "xs.threshold"}
] $init_edge
# Move was a no-op (single tile already at column 0), and apply-move returns
# the same tiles -- so the emitted state matches; undo finds the stack at
# its bottom, echoes; threshold emits the same state.
let r6_final = $r6 | where threshold == true | first
assert (tiles-equal $r6_final.state.tiles $edge.tiles) "no-op move + undo round-trips"

# 7. A slam (multi-step shift) counts as ONE undo step: undo after a slam
#    restores the pre-slam state. Construct a board where slam-l merges
#    two pairs, then undo brings everything back. Use a single-seed slam
#    so this stays fast (the pacing stage sleeps 250ms between paced items,
#    and there is only one here so no sleep fires).
let pair = {
  tiles: [
    {id: 1 r: 0 c: 0 value: 2}
    {id: 2 r: 0 c: 1 value: 2}
  ]
  next_id: 3 score: 0 game_over: false
}
let init_pair = {stack: [$pair] mode: "game"}
let slam_frames = [
  {topic: "game.t.move" meta: {intent: "slam-h" seeds: [{idx: 0 value: 0}]}}
  {topic: "game.t.move" meta: {kind: "undo"}}
  {topic: "xs.threshold"}
]
let r7 = drive $slam_frames $init_pair
let r7_final = $r7 | where threshold == true | first
assert (tiles-equal $r7_final.state.tiles $pair.tiles) "undo after slam restores pre-slam state"

print "examples/2048/test.nu: all assertions passed"
