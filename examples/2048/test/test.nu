use std/assert

# Import the pure pieces of the 2048 module: roll, slide-tiles,
# apply-move, tiles-equal, initial-state, impulses-to-states, ... The
# store-touching helpers (frames-to-states, list-games, ...) aren't
# exercised here -- those are covered by the browser e2e.
const script_dir = path self | path dirname
use ($script_dir | path join ".." "tfe" "game.nu") *
use ($script_dir | path join ".." "tfe" "sse.nu") *

# A fixed game id so every test is deterministic.
const GID = "test-game-aaaa"

# --- roll: deterministic hash -> seeds --------------------------------------

# Regression: `into int --radix 16` errors on hex strings starting with "0b"
# (interpreted as a binary literal prefix), and similarly for "0x" / "0o".
# `game-249` happens to produce a hash starting with "0b" -- roll must still
# return clean integers, not error.
let r = roll "game-249" {tiles: [] next_id: 1 score: 0 game_over: false} "h"
assert (($r.idx | describe) == "int") "roll returns int idx even on 0b-prefixed hash"
assert ($r.value >= 0 and $r.value <= 9) "roll value in 0..9"

# Same inputs always yield the same outputs.
let r2 = roll "game-249" {tiles: [] next_id: 1 score: 0 game_over: false} "h"
assert ($r.idx == $r2.idx and $r.value == $r2.value) "roll is deterministic"

# --- pure game logic --------------------------------------------------------

# initial-state seeds two tiles deterministically from the game_id.
let s0 = initial-state $GID
assert (($s0.tiles | length) == 2) "initial state has 2 tiles"
assert ($s0.score == 0) "initial score 0"
assert (not $s0.game_over) "initial not game over"
assert ($s0.next_id >= 3) "next_id advanced past two spawned tiles"

# Calling initial-state with the same game_id reproduces the same board.
let s0_again = initial-state $GID
assert (tiles-equal $s0.tiles $s0_again.tiles) "initial-state is deterministic per game_id"

# Different game_ids generally produce different starts.
let s0_other = initial-state "other-game-bbbb"
assert (not (tiles-equal $s0.tiles $s0_other.tiles)) "different game_id -> different initial tiles"

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
let post = $pre | apply-move "h" $GID
let moved = $post.tiles | where id == 1 | first
assert ($moved.c == 0) "single tile moves to column 0 on left"
assert (($post.tiles | length) == 2) "spawn-tile added a second tile after the move"

# Same input twice produces the same output -- the roll is deterministic.
let post_again = $pre | apply-move "h" $GID
assert (tiles-equal $post.tiles $post_again.tiles) "apply-move is deterministic for (state, dir, game_id)"

# apply-move is identity (same tiles) when the move doesn't change the board.
let locked = {
  tiles: [{id: 1 r: 0 c: 0 value: 2}]
  next_id: 2 score: 0 game_over: false
}
let noop = $locked | apply-move "h" $GID
assert (tiles-equal $locked.tiles $noop.tiles) "no-op move returns same tile list"
assert ($locked.score == $noop.score) "no-op move keeps score"

# tiles-equal is order-independent (sort-by id).
let a = [{id: 1 r: 0 c: 0 value: 2} {id: 2 r: 1 c: 1 value: 4}]
let b = [{id: 2 r: 1 c: 1 value: 4} {id: 1 r: 0 c: 0 value: 2}]
assert (tiles-equal $a $b) "tiles-equal ignores order"
let c = [{id: 1 r: 0 c: 0 value: 2} {id: 2 r: 1 c: 1 value: 8}]
assert (not (tiles-equal $a $c)) "tiles-equal detects value diff"

# A row of 2s slides right and merges; rightmost cell holds the merged 4.
let diag = {
  tiles: [
    {id: 1 r: 0 c: 0 value: 2}
    {id: 2 r: 0 c: 1 value: 2}
    {id: 3 r: 0 c: 2 value: 2}
    {id: 4 r: 0 c: 3 value: 2}
  ]
  next_id: 5 score: 0 game_over: false
}
let after_l = $diag | apply-move "l" $GID
let rightmost = $after_l.tiles | where r == 0 and c == 3 | first
assert ($rightmost.value == 4) "right-side merge yields 4 on slide right"

# --- filter-for-player ------------------------------------------------------

let frames = [
  {topic: "player.alice.games"}        # alice's index -- keep
  {topic: "game.abc.move"}             # any game move topic -- keep
  {topic: "game.xyz.move"}             # any game move topic -- keep
  {topic: "xs.threshold"}              # threshold marker -- keep
  {topic: "player.bob.games"}          # bob's index -- drop
  {topic: "templates.html"}            # unrelated -- drop
  {topic: "game.abc"}                  # missing .move suffix -- drop
  {topic: "abc.move"}                  # missing game. prefix -- drop
]
let kept = $frames | filter-for-player "player.alice.games" | get topic
let expected = ["player.alice.games" "game.abc.move" "game.xyz.move" "xs.threshold"]
assert ($kept == $expected) $"filter kept ($kept), expected ($expected)"

# --- impulses-to-states stack discipline ------------------------------------

# A known starting state so we can reason about the stack precisely.
let known = {
  tiles: [
    {id: 1 r: 0 c: 1 value: 2}
    {id: 2 r: 0 c: 2 value: 2}
  ]
  next_id: 3 score: 0 game_over: false
}
let GAMES_TOPIC = $"player.test-pid.games"
let MOVE_TOPIC = $"game.($GID).move"
let init = {stack: [$known] mode: "game" game_id: $GID games_topic: $GAMES_TOPIC}

# Helper: feed a list of frames through impulses-to-states and return the
# emitted records as a list. Each frame emits one or more records.
def drive [frames: list, initial: record]: nothing -> list {
  $frames | impulses-to-states $initial
}

# 0. Empty log: only an xs.threshold marker, no prior frames. The pipeline
#    must produce ONE state record (the placeholder), not crash. Regression
#    for prod incident where threshold-gate emitted null and states-to-html
#    errored on `$s.state`.
let placeholder_init = {
  stack: [{tiles: [] next_id: 1 score: 0 game_over: false}]
  mode: "game" game_id: "" games_topic: "player.test-pid.games"
}
let r0 = [{topic: "xs.threshold"}] | impulses-to-states $placeholder_init | threshold-gate-states | take 2
# Threshold emits: (1) the gated state record, (2) a {signals: {replayMs}}
# debug record. The state record comes first and has zero tiles.
let r0_state = $r0 | where {|i| ($i | get state? | default null) != null} | first
assert (($r0_state.state.tiles | length) == 0) "empty-log state has zero tiles"
assert ((($r0_state | get threshold?) | default false) == false) "threshold flag stripped before downstream"
let r0_signals = $r0 | where {|i| ('signals' in $i)} | first
assert (($r0_signals.signals | get replayMs? | default null) != null) "threshold emits replayMs signals"

# 2. xs.threshold marker passes the current top of stack through.
let r2 = drive [{topic: "xs.threshold"}] $init
let r2_first = $r2 | first
assert (($r2_first | get threshold) == true) "threshold marker carries threshold=true"
assert (tiles-equal $r2_first.state.tiles $known.tiles) "threshold emits current top"

# 3. A move that changes the board emits one state.
let r3 = drive [{topic: "game.test-game-aaaa.move" meta: {intent: "h"}}] $init
let r3_state = $r3 | first | get state
let changed = not (tiles-equal $r3_state.tiles $known.tiles)
assert $changed "left move on known state changes the board"
assert (($r3 | first | get direction) == "h") "move record carries direction"

# 4. Undo after a real move restores the pre-move state.
let move_then_undo = [
  {topic: "game.test-game-aaaa.move" meta: {intent: "h"}}
  {topic: "game.test-game-aaaa.move" meta: {kind: "undo"}}
  {topic: "xs.threshold"}
]
let r4 = drive $move_then_undo $init
let r4_final = $r4 | where threshold == true | first
assert (tiles-equal $r4_final.state.tiles $known.tiles) "undo restores the prior state"

# 5. Undo + same direction reproduces the original spawn (the whole point
#    of game_id-based deterministic rolls).
let move_undo_redo = [
  {topic: "game.test-game-aaaa.move" meta: {intent: "h"}}
  {topic: "game.test-game-aaaa.move" meta: {kind: "undo"}}
  {topic: "game.test-game-aaaa.move" meta: {intent: "h"}}
  {topic: "xs.threshold"}
]
let r5 = drive $move_undo_redo $init
let r5_final = $r5 | where threshold == true | first
let r3_final_again = drive [{topic: "game.test-game-aaaa.move" meta: {intent: "h"}} {topic: "xs.threshold"}] $init | where threshold == true | first
assert (tiles-equal $r5_final.state.tiles $r3_final_again.state.tiles) "undo + redo same dir = same board"

# 6. Undo at the bottom of the stack is a no-op echo (state unchanged).
let r6 = drive [{topic: "game.test-game-aaaa.move" meta: {kind: "undo"}}] $init
assert (tiles-equal ($r6 | first | get state | get tiles) $known.tiles) "undo at bottom echoes current"

# 7. A no-op move does NOT push, so a subsequent undo finds the stack at
#    its bottom and echoes.
let edge = {
  tiles: [{id: 1 r: 0 c: 0 value: 2}]
  next_id: 2 score: 0 game_over: false
}
let init_edge = {stack: [$edge] mode: "game" game_id: $GID games_topic: $GAMES_TOPIC}
let r7 = drive [
  {topic: "game.test-game-aaaa.move" meta: {intent: "h"}}
  {topic: "game.test-game-aaaa.move" meta: {kind: "undo"}}
  {topic: "xs.threshold"}
] $init_edge
let r7_final = $r7 | where threshold == true | first
assert (tiles-equal $r7_final.state.tiles $edge.tiles) "no-op move + undo round-trips"

# 8. Legacy slam-X intents (no longer supported) fall through to the noop
#    echo arm: state unchanged, no stack push.
let pair = {
  tiles: [
    {id: 1 r: 0 c: 0 value: 2}
    {id: 2 r: 0 c: 1 value: 2}
  ]
  next_id: 3 score: 0 game_over: false
}
let init_pair = {stack: [$pair] mode: "game" game_id: $GID games_topic: $GAMES_TOPIC}
let r8 = drive [{topic: "game.test-game-aaaa.move" meta: {intent: "slam-h"}}] $init_pair
assert (tiles-equal ($r8 | first | get state | get tiles) $pair.tiles) "legacy slam-X is a no-op echo"

# 9. A frame on the player's games_topic starts a new game: stack resets,
#    game_id updates, and the emitted state is the fresh initial-state.
let r9 = drive [{topic: $GAMES_TOPIC id: "new-game-zzzz" meta: {}}] $init
let r9_state = $r9 | first | get state
let expected9 = (initial-state "new-game-zzzz") | get tiles
assert (tiles-equal $r9_state.tiles $expected9) "new game frame resets to initial-state(new id)"

# 10. After a game switch, OLD game's move frames are dropped: state stays
#     at the new game's initial board (the old game's "h" intent has no
#     effect because impulses-to-states only matches `game.<current>.move`).
let r10 = drive [
  {topic: $GAMES_TOPIC id: "new-game-zzzz" meta: {}}
  {topic: $"game.($GID).move" meta: {intent: "h"}}    # old game -- dropped
  {topic: "xs.threshold"}
] $init
let r10_final = $r10 | where threshold == true | first
let new_init_tiles = (initial-state "new-game-zzzz") | get tiles
assert (tiles-equal $r10_final.state.tiles $new_init_tiles) "stale game's move is dropped after switch"

# --- frames-to-states ------------------------------------------------------
#
# Contract: every `game.<id>.move` frame -- empty-intent ping, h/j/k/l,
# undo -- must emit a state record carrying the originating req_id, so
# the client's pending RTT probe resolves whether or not the actor
# follows up with a snapshot frame.

let MOVE_TOPIC2 = $"game.($GID).move"
let SNAP_TOPIC  = $"game.($GID).snapshot"

# Helper: pump a single frame through frames-to-states and return the
# non-acc-init records (filter out the {next: ...} accumulator entries).
def drive-frames [frames: list]: nothing -> list {
  $frames | frames-to-states
}

# 11. RTT-ping move frame (intent="") echoes current state with req_id.
let r11 = drive-frames [{id: "m1" topic: $MOVE_TOPIC2 meta: {intent: "" req_id: "ping-1"}}]
assert (($r11 | length) == 1) "intent=\"\" emits exactly one record"
assert (($r11 | first | get req_id) == "ping-1") "RTT ping echoes req_id"

# 12. h/j/k/l move frames echo with req_id too, so the RTT probe
#     resolves even on no-op moves (where the actor writes no snapshot).
let r12 = drive-frames [{id: "m2" topic: $MOVE_TOPIC2 meta: {intent: "h" req_id: "probe-42"}}]
assert (($r12 | length) == 1) "h-move emits exactly one record"
assert (($r12 | first | get req_id) == "probe-42") "h-move echoes req_id (NO-OP RTT resolution)"

# 13. Undo move frame also echoes (kind=undo, intent unset).
let r13 = drive-frames [{id: "m3" topic: $MOVE_TOPIC2 meta: {kind: "undo" req_id: "undo-7"}}]
assert (($r13 | length) == 1) "undo move emits exactly one record"
assert (($r13 | first | get req_id) == "undo-7") "undo move echoes req_id"

# 14. A snapshot frame emits a state record carrying its meta.req_id and
#     the snapshot's state. This is the normal "real move" path -- the
#     echo from #12 lands first; this one then re-renders with new tiles.
let snap_state = {tiles: [{id: 1 r: 0 c: 0 value: 4}] next_id: 2 score: 4 game_over: false ghosts: []}
let r14 = drive-frames [{id: "s1" topic: $SNAP_TOPIC meta: {state: $snap_state req_id: "probe-42" intent: "h"}}]
assert (($r14 | length) == 1) "snapshot emits one record"
assert (tiles-equal ($r14 | first | get state | get tiles) $snap_state.tiles) "snapshot state propagates"
assert (($r14 | first | get req_id) == "probe-42") "snapshot carries req_id"

# 15. Pre-converted SSE event records (e.g. from pulse-keepalive) flow
#     through unchanged so downstream stages can dispatch by `event`.
let evt = {event: "datastar-patch-signals" data: ["signals {}"]}
let r15 = drive-frames [$evt]
assert (($r15 | first) == $evt) "event records pass through unchanged"

print "examples/2048/test.nu: all assertions passed"
