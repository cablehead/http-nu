# Benchmark for the slide-tiles hot path: compare three implementations.
#
#   1. CURRENT  -- the existing slide-tiles in mod.nu (rotate-and-unwind).
#   2. DIRECT   -- same tile-list shape, but each direction is dispatched
#                  directly to a per-line walk; no reflect/transpose passes.
#   3. FLAT     -- state shape changes: cells = 16-element list of values
#                  (0 = empty), cell_ids = parallel 16-element list of ids.
#                  Slide is integer-indexed walks. Identity preserved via
#                  the parallel cell_ids list.
#
# Per-direction equivalence is asserted for #1 vs #2 (same shape).
# For #3 we verify that slide-flat -> from-flat produces the same tile set
# as slide-tiles (ids and positions match).
#
# Run:
#   $env.XS_ADDR = (realpath ./store)
#   nu examples/2048/bench.nu

overlay use ~/xs/xs.nu
const BENCH_DIR = path self | path dirname
overlay use -r ($BENCH_DIR | path join "mod.nu") as 2048

const GAME_ID = "03g4runmv67s2pc97gf1lk2me"
const N = 400

# ============================================================================
# Approach 2 (DIRECT): tile-list state, per-direction dispatch (no rotate)
# ============================================================================

const DIRECT_PLAN = {
  h: {group_field: "r", motion_field: "c", asc: true,  start: 0, step: 1}
  l: {group_field: "r", motion_field: "c", asc: false, start: 3, step: -1}
  k: {group_field: "c", motion_field: "r", asc: true,  start: 0, step: 1}
  j: {group_field: "c", motion_field: "r", asc: false, start: 3, step: -1}
}

# Slide a single sorted line, packing in `motion_field` from `start` by `step`,
# merging adjacent equal-valued pairs (the leading id wins, value doubles).
def walk-merge-line [
  sorted: list
  fixed_field: string
  fixed_value: int
  motion_field: string
  start: int
  step: int
]: nothing -> record {
  mut out = []
  mut score = 0
  mut pos = $start
  mut i = 0
  let n = $sorted | length
  while $i < $n {
    let cur = $sorted | get $i
    let has_next = ($i + 1) < $n
    let merged = $has_next and ($cur.value == ($sorted | get ($i + 1) | get value))
    let v = if $merged { $cur.value * 2 } else { $cur.value }
    let new_tile = ({id: $cur.id value: $v}
      | upsert $fixed_field $fixed_value
      | upsert $motion_field $pos)
    $out = $out | append $new_tile
    if $merged {
      $score = $score + $v
      $i = $i + 2
    } else {
      $i = $i + 1
    }
    $pos = $pos + $step
  }
  {tiles: $out, score: $score}
}

def slide-direct [dir: string]: list -> record {
  let p = $DIRECT_PLAN | get -o $dir
  if $p == null { return {tiles: $in, score: 0} }
  let by_line = $in | group-by {|t| $t | get ($p.group_field) | into string}
  let lines = 0..3 | each {|i|
    let line_tiles = ($by_line | get -o ($i | into string) | default [])
    let motion_field = $p.motion_field
    let sorted = if $p.asc {
      $line_tiles | sort-by {|t| $t | get $motion_field}
    } else {
      $line_tiles | sort-by --reverse {|t| $t | get $motion_field}
    }
    walk-merge-line $sorted ($p.group_field) $i ($p.motion_field) $p.start $p.step
  }
  {
    tiles: ($lines | each { $in.tiles } | flatten)
    score: ($lines | each { $in.score } | math sum)
  }
}

# ============================================================================
# Approach 3 (FLAT): cells + parallel cell_ids, integer-indexed
# ============================================================================

const ZEROS_16 = [0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0]

# Lines for each direction, in motion order. Each is a list of cell indices.
# Reading along a line in this order, then packing back in this order, slides
# tiles in the chosen direction.
const FLAT_LINES = {
  h: [[0 1 2 3]   [4 5 6 7]    [8 9 10 11]  [12 13 14 15]]
  l: [[3 2 1 0]   [7 6 5 4]    [11 10 9 8]  [15 14 13 12]]
  k: [[0 4 8 12]  [1 5 9 13]   [2 6 10 14]  [3 7 11 15]]
  j: [[12 8 4 0]  [13 9 5 1]   [14 10 6 2]  [15 11 7 3]]
}

def to-flat []: record -> record {
  let s = $in
  mut cells = $ZEROS_16
  mut ids = $ZEROS_16
  for t in $s.tiles {
    let idx = $t.r * 4 + $t.c
    $cells = $cells | update $idx $t.value
    $ids = $ids | update $idx $t.id
  }
  {cells: $cells, cell_ids: $ids, next_id: $s.next_id, score: $s.score, game_over: $s.game_over}
}

def from-flat []: record -> record {
  let f = $in
  mut tiles = []
  for i in 0..15 {
    let v = $f.cells | get $i
    if $v != 0 {
      $tiles = $tiles | append {
        id: ($f.cell_ids | get $i)
        r: ($i // 4)
        c: ($i mod 4)
        value: $v
      }
    }
  }
  {tiles: $tiles, next_id: $f.next_id, score: $f.score, game_over: $f.game_over}
}

def slide-flat [dir: string]: record -> record {
  let state = $in
  let lines = $FLAT_LINES | get -o $dir
  if $lines == null { return $state }
  mut new_cells = $state.cells
  mut new_ids = $state.cell_ids
  mut score_delta = 0
  for line in $lines {
    # Read line values + ids in motion order. Closures can't capture `mut`
    # vars, so bind to immutable locals first.
    let cells_snap = $new_cells
    let ids_snap = $new_ids
    let vals = $line | each {|i| $cells_snap | get $i}
    let ids  = $line | each {|i| $ids_snap | get $i}
    # Compact (drop zeros), preserving motion order.
    mut packed_v = []
    mut packed_i = []
    for k in 0..3 {
      let v = $vals | get $k
      if $v != 0 {
        $packed_v = $packed_v | append $v
        $packed_i = $packed_i | append ($ids | get $k)
      }
    }
    # Merge adjacent equal pairs (leading id wins, value doubles).
    mut out_v = []
    mut out_i = []
    mut j = 0
    let pn = $packed_v | length
    while $j < $pn {
      let v = $packed_v | get $j
      if ($j + 1) < $pn and ($packed_v | get ($j + 1)) == $v {
        $out_v = $out_v | append ($v * 2)
        $out_i = $out_i | append ($packed_i | get $j)
        $score_delta = $score_delta + ($v * 2)
        $j = $j + 2
      } else {
        $out_v = $out_v | append $v
        $out_i = $out_i | append ($packed_i | get $j)
        $j = $j + 1
      }
    }
    # Pad with zeros to 4 elements.
    while ($out_v | length) < 4 {
      $out_v = $out_v | append 0
      $out_i = $out_i | append 0
    }
    # Write back at line positions.
    for k in 0..3 {
      let dest = $line | get $k
      $new_cells = $new_cells | update $dest ($out_v | get $k)
      $new_ids = $new_ids | update $dest ($out_i | get $k)
    }
  }
  $state | merge {
    cells: $new_cells
    cell_ids: $new_ids
    score: ($state.score + $score_delta)
  }
}

# ============================================================================
# Benchmark
# ============================================================================

print $"benchmark: ($N) iterations on a mid-game state from ($GAME_ID)"
let mid_state = (.cat -T $"game.($GAME_ID).move" | first 200 | project-game $GAME_ID)
let mid_flat  = ($mid_state | to-flat)
print $"mid state: tiles=($mid_state.tiles | length), score=($mid_state.score)"

# Correctness: DIRECT vs CURRENT (same shape, ids must match exactly)
print ""
print "--- correctness: DIRECT vs CURRENT ---"
for dir in [h l k j] {
  let r_cur = ($mid_state.tiles | slide-tiles $dir)
  let r_dir = ($mid_state.tiles | slide-direct $dir)
  let ids_match = (tiles-equal $r_cur.tiles $r_dir.tiles)
  let score_match = ($r_cur.score == $r_dir.score)
  print $"  ($dir): score=($r_cur.score)=($r_dir.score) [match=($score_match)], tiles match=($ids_match)"
}

# Correctness: FLAT vs CURRENT
print ""
print "--- correctness: FLAT (slide+from-flat) vs CURRENT ---"
for dir in [h l k j] {
  let r_cur = ($mid_state.tiles | slide-tiles $dir)
  let r_flat = ($mid_flat | slide-flat $dir | from-flat)
  let ids_match = (tiles-equal $r_cur.tiles $r_flat.tiles)
  let score_delta = ($r_flat.score - $mid_state.score)
  let score_match = ($r_cur.score == $score_delta)
  print $"  ($dir): score=($r_cur.score)=($score_delta) [match=($score_match)], tiles match=($ids_match)"
}

# Per-call timings, all four directions.
print ""
print $"--- per-direction timings: ($N) calls each ---"
for dir in [h l k j] {
  print $"  dir ($dir):"
  let t_cur = (timeit { 1..$N | each {|_| $mid_state.tiles | slide-tiles $dir} | last | get score})
  let t_dir = (timeit { 1..$N | each {|_| $mid_state.tiles | slide-direct $dir} | last | get score})
  let t_flat_pure = (timeit { 1..$N | each {|_| $mid_flat | slide-flat $dir} | last | get score})
  let t_flat_conv = (timeit { 1..$N | each {|_| $mid_state | to-flat | slide-flat $dir | from-flat} | last | get score})
  print $"    CURRENT      : ($t_cur)"
  print $"    DIRECT       : ($t_dir)"
  print $"    FLAT pure    : ($t_flat_pure)"
  print $"    FLAT +conv   : ($t_flat_conv)"
}
