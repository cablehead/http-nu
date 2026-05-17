# Benchmark for the 2048 example's hot path. Times the per-call cost of the
# pure game functions (slide-tiles, apply-move, spawn-tile, roll, etc.) and
# the per-frame cost of the streaming projector. No --store required: the
# benchmark builds a synthetic mid-game state by playing a fixed sequence
# of moves against a fresh game.
#
# Run:
#   nu examples/2048/test/bench.nu

const BENCH_DIR = path self | path dirname
overlay use -r ($BENCH_DIR | path join ".." "tfe" "game.nu") as twentyfortyeight

const N = 400
const GAME_ID = "bench-game"

# Build a synthetic mid-game state by applying a deterministic sequence of
# moves. Plays to a board with several non-trivial tiles so that slide
# operations have real work to do.
def synthetic-state []: nothing -> record {
  let dirs = [j h k l j h l k j h k l]
  mut s = (initial-state $GAME_ID)
  for d in $dirs {
    $s = ($s | apply-move $d $GAME_ID)
  }
  $s
}

let mid = synthetic-state
print $"synthetic mid state: tiles=($mid.tiles | length), score=($mid.score)"

print ""
print $"--- per-call timings: ($N) iterations ---"
def t [label: string action: closure] {
  print $"  ($label): (do $action)"
}

t "slide-tiles \"h\"" { timeit { 1..$N | each {|_| $mid.tiles | slide-tiles "h" } | last | get score } }
t "slide-tiles \"l\"" { timeit { 1..$N | each {|_| $mid.tiles | slide-tiles "l" } | last | get score } }
t "slide-tiles \"k\"" { timeit { 1..$N | each {|_| $mid.tiles | slide-tiles "k" } | last | get score } }
t "slide-tiles \"j\"" { timeit { 1..$N | each {|_| $mid.tiles | slide-tiles "j" } | last | get score } }
t "apply-move \"h\""  { timeit { 1..$N | each {|_| $mid | apply-move "h" $GAME_ID } | last | get score } }
t "apply-move \"j\""  { timeit { 1..$N | each {|_| $mid | apply-move "j" $GAME_ID } | last | get score } }
let seeds = (roll $GAME_ID $mid "h")
t "spawn-tile"        { timeit { 1..$N | each {|_| $mid | spawn-tile $seeds } | last | get next_id } }
t "roll"              { timeit { 1..$N | each {|_| roll $GAME_ID $mid "h" } | last } }
t "is-game-over"      { timeit { 1..$N | each {|_| $mid | is-game-over } | last } }
t "tiles-equal"       { timeit { 1..$N | each {|_| tiles-equal $mid.tiles $mid.tiles } | last } }

print ""
print $"--- end-to-end fold: project-game over a synthetic move stream ---"
# Build a list of synthetic move frames (no real frame ids; impulses-to-states
# tolerates missing ids via `get id?`).
def frames-for [n: int] {
  1..$n | each {|i|
    let dir = ([h j k l] | get (($i + ($i * 7)) mod 4))
    {topic: $"game.($GAME_ID).move" meta: {intent: $dir req_id: ""}}
  }
}
for n in [100 200 400 800] {
  let frames = (frames-for $n)
  let elapsed = (timeit { $frames | project-game $GAME_ID | get score })
  print $"  ($n) frames: ($elapsed)"
}
