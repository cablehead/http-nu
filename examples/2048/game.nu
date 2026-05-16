# Pure 2048 game logic + the replay-pipeline state machine.
#
# No store dependencies (.cat, .last, .append) live here -- those go in
# mod.nu, which `use`s this module and adds them. The pure layer is what
# the xs snapshot-actor uses via `use game *` (modules registered to xs
# topics get parsed before xs's store-binding commands are available, so
# they have to stay pure).

# --- game logic -----------------------------------------------------------

# Deterministic random "roll" -- a pure function of (game_id, state, key).
# Same inputs always yield the same {idx, value}, so the whole game replays
# identically and (undo + same direction) reproduces the original spawn.
# Frames carry only the intent and game_id; replay re-derives every spawn.
export def roll [game_id: string, state: record, key: string]: nothing -> record {
  let payload = $"($game_id)|($key)|" + ($state | to json --raw)
  let h = $payload | hash sha256
  # Prepend "0x" so `into int` parses as hex. `--radix 16` would mis-read
  # hashes starting with "0b" / "0o" as binary / octal literals and error.
  {
    idx: ($h | str substring 0..8 | $"0x($in)" | into int)
    value: (($h | str substring 8..10 | $"0x($in)" | into int) mod 10)
  }
}

# Drop a 2 (90%) or 4 (10%) into an empty cell, picked by a roll.
#
# Empty cells are derived as integer indices (0..15, where idx = r*4 + c)
# rather than {r, c} records, to avoid constructing 16 fresh records every
# spawn -- profiling showed that grid-build dominated this function.
export def spawn-tile [seeds: record]: record -> record {
  let s = $in
  let occupied = $s.tiles | each {|t| $t.r * 4 + $t.c }
  let empties = 0..15 | where {|i| $i not-in $occupied }
  if ($empties | is-empty) { return $s }
  let pick_idx = $empties | get ($seeds.idx mod ($empties | length))
  let value = if ($seeds.value == 0) { 4 } else { 2 }
  $s
  | update tiles { append {id: $s.next_id r: ($pick_idx // 4) c: ($pick_idx mod 4) value: $value} }
  | update next_id { $in + 1 }
}

# Initial board for a game. Two tiles spawned via roll() with key "@init".
export def initial-state [game_id: string]: nothing -> record {
  let s0 = {tiles: [] next_id: 1 score: 0 game_over: false}
  let s1 = $s0 | spawn-tile (roll $game_id $s0 "@init")
  $s1 | spawn-tile (roll $game_id $s1 "@init")
}

# Slide one row left, preserving tile identity. When two adjacent tiles merge,
# the leading tile keeps its id (and doubles its value); the trailing tile is
# removed. Returns {tiles, score}.
#
# Imperative `mut`+`while` rather than `reduce`: profiling showed reduce was
# ~75% slower in this hot path because every step paid closure-call cost
# plus an immutable record copy for the accumulator.
export def slide-row-tiles [row_idx: int]: list -> record {
  let in_row = $in | sort-by c
  mut out = []
  mut score = 0
  mut col = 0
  mut i = 0
  let n = $in_row | length
  while $i < $n {
    let cur = $in_row | get $i
    let has_next = ($i + 1) < $n
    let nxt = if $has_next { $in_row | get ($i + 1) } else { null }
    if $has_next and $cur.value == $nxt.value {
      let merged = $cur.value * 2
      $out = $out | append {id: $cur.id r: $row_idx c: $col value: $merged}
      $score = $score + $merged
      $col = $col + 1
      $i = $i + 2
    } else {
      $out = $out | append {id: $cur.id r: $row_idx c: $col value: $cur.value}
      $col = $col + 1
      $i = $i + 1
    }
  }
  {tiles: $out score: $score}
}

# Group tiles by row once, slide each row, flatten back. The previous form
# called `where r == X` four times over the full tile list -- redundant
# work that profiling showed cost ~40% of this function's time.
export def slide-tiles-left []: list -> record {
  let by_row = $in | group-by r
  let rows = 0..3 | each {|r|
    ($by_row | get -o ($r | into string) | default []) | slide-row-tiles $r
  }
  {
    tiles: ($rows | each { $in.tiles } | flatten)
    score: ($rows | each { $in.score } | math sum)
  }
}

# Reflect tiles over the vertical axis (c -> 3 - c).
export def reflect-cols []: list -> list {
  $in | each {|t| $t | upsert c (3 - $t.c) }
}

# Swap r and c (transpose over the diagonal).
export def transpose-tiles []: list -> list {
  $in | each {|t| $t | upsert r $t.c | upsert c $t.r }
}

# All four directions reuse `slide-tiles-left` by reflecting/transposing in
# and back out -- so the merge logic lives in exactly one place. Direction
# is data: each dir maps to {pre, post} closures that frame slide-tiles-left.
# The post-transform is the inverse of the pre-transform (reflect is its own
# inverse; transpose is its own inverse; the composition reverses on the way
# out).
export def slide-tiles [dir: string]: list -> record {
  let plan = {
    h: {pre: {|| $in}                              post: {|| $in}}
    l: {pre: {|| reflect-cols}                     post: {|| reflect-cols}}
    k: {pre: {|| transpose-tiles}                  post: {|| transpose-tiles}}
    j: {pre: {|| transpose-tiles | reflect-cols}   post: {|| reflect-cols | transpose-tiles}}
  } | get -o $dir
  if $plan == null { return {tiles: $in score: 0} }
  let r = $in | do $plan.pre | slide-tiles-left
  {tiles: ($r.tiles | do $plan.post) score: $r.score}
}

export def tiles-equal [a: list b: list]: nothing -> bool {
  ($a | sort-by id) == ($b | sort-by id)
}

export def is-game-over []: record -> bool {
  let s = $in
  if ($s.tiles | length) < 16 { return false }
  let mergeable = $s.tiles | any {|t|
      let right = $t.c < 3 and ($s.tiles | where r == $t.r and c == ($t.c + 1) | first | get value) == $t.value
      let down = $t.r < 3 and ($s.tiles | where r == ($t.r + 1) and c == $t.c | first | get value) == $t.value
      $right or $down
    }
  not $mergeable
}

export def apply-move [dir: string, game_id: string]: record -> record {
  let s = $in
  let r = $s.tiles | slide-tiles $dir
  if $s.game_over or (tiles-equal $s.tiles $r.tiles) { return $s }
  let next = ($s
    | update tiles { $r.tiles }
    | update score { $in + $r.score }
    | spawn-tile (roll $game_id $s $dir))
  $next | upsert game_over ($next | is-game-over)
}

# --- replay pipeline ------------------------------------------------------

# Static topic filter: admits this player's games index, any game's move
# topic, and the xs.threshold marker. Everything else (other players'
# topics, unrelated streams) is dropped. impulses-to-states still does
# the dynamic per-game filtering -- this stage is the cheap pre-filter.
export def filter-for-player [games_topic: string] {
  where {|f| (
    $f.topic == $games_topic
    or (($f.topic | str starts-with "game.") and ($f.topic | str ends-with ".move"))
    or $f.topic == "xs.threshold"
    or $f.topic == "xs.pulse"
  ) }
}

# Takes xs frames, yields {state, mode, threshold?} records. One emit per
# move frame.
export def impulses-to-states [initial: record] {
  # s = {stack: [state, ...], mode, game_id, games_topic}
  # Stack discipline: every move that actually changes the board pushes the
  # resulting state. An undo frame pops the top, exposing the previous state.
  # The "current" state is always the top of the stack. game_id seeds spawn
  # determinism so (state, dir) -> same spawn within a game; undo + same-dir
  # = same outcome. A frame on the player's games_topic starts a new game:
  # game_id becomes the frame's id, stack resets to a fresh initial-state.
  # Move frames are only processed for the current game's topic; everything
  # else is dropped. Legacy slam-X intents in old logs fall through to the
  # noop-echo arm at the bottom.
  generate {|frame s|
    let cur = $s.stack | last
    if $frame.topic == "xs.threshold" {
      # Replay caught up to live. Also emit a signals patch with how long
      # the replay took -- useful as a quick debug indicator client-side.
      let started = $s | get started? | default (date now)
      let elapsed = ((date now) - $started) / 1ms | math round
      return {out: [
        {state: $cur, mode: $s.mode, threshold: true}
        {signals: {replayMs: $elapsed}}
      ], next: $s}
    }
    if $frame.topic == "xs.pulse" {
      # SSE keepalive -- emit a pulse marker; downstream stages turn it into
      # a datastar-patch-signals no-op so the client sees a sign of life.
      return {out: [{pulse: true, mode: $s.mode}], next: $s}
    }
    if $frame.topic == $s.games_topic {
      # New game (or first game) for this player. Reset to a fresh board.
      # Carry req_id from the originating reset POST so the client's pending
      # RTT probe finds a matching mutation and resolves.
      let new_game_id = $frame.id
      let new_state = initial-state $new_game_id
      let req_id = $frame | get meta? | default {} | get req_id? | default ""
      return {
        out: [{state: $new_state, mode: $s.mode, threshold: false, req_id: $req_id}]
        next: ($s | update stack [$new_state] | update game_id $new_game_id)
      }
    }
    if $frame.topic != $"game.($s.game_id).move" {
      # Some other player's game, or our own old game. Drop.
      return {next: $s}
    }
    let kind = $frame.meta | get kind? | default "move"
    if $kind == "view" {
      # Ephemeral (ttl=ephemeral): only arrives live, never during replay,
      # so mode resets to game on reconnect.
      let new_s = $s | upsert mode ($frame.meta | get mode? | default "game")
      return {out: [{state: $cur, mode: $new_s.mode, threshold: false}], next: $new_s}
    }
    let req_id = $frame.meta | get req_id? | default ""
    if $kind == "undo" {
      # Pop one entry. If only the initial state is on the stack, echo.
      if ($s.stack | length) <= 1 {
        return {out: [{state: $cur, mode: $s.mode, req_id: $req_id, threshold: false}], next: $s}
      }
      let popped = $s.stack | drop 1
      let new_top = $popped | last
      return {out: [{state: $new_top, mode: $s.mode, direction: "undo", changed: true, req_id: $req_id, threshold: false, move_id: ($frame | get id? | default "")}], next: ($s | update stack $popped)}
    }
    let intent = $frame.meta | get intent? | default ""
    if $intent in [h j k l] {
      let new_state = $cur | apply-move $intent $s.game_id
      let changed = not (tiles-equal $cur.tiles $new_state.tiles)
      # No-op moves don't push -- nothing to undo if the board didn't change.
      let new_stack = if $changed { $s.stack | append $new_state } else { $s.stack }
      return {out: [{state: $new_state, mode: $s.mode, direction: $intent, changed: $changed, req_id: $req_id, threshold: false, move_id: ($frame | get id? | default "")}], next: ($s | update stack $new_stack)}
    }
    # Any other intent (empty ping, legacy slam-X, unrecognised) emits a no-op state
    # echo. The client uses the resulting mutation to measure RTT and to
    # clear the "lit edge" -- if the edge stays lit, the server is slow.
    return {out: [{state: $cur, mode: $s.mode, req_id: $req_id, threshold: false}], next: $s}
  } $initial
  | flatten
}

# Pipeline form: fold a stream of move frames for one game into final state.
# game_id is required -- spawn rolls are seeded from it, so without it the
# replay would diverge from what the live server produced.
#
#   .cat -T $"game.($id).move" | project-game $id | to json
export def project-game [game_id: string]: list -> record {
  # Single-statement body: a `let` between a custom command's input and a
  # downstream `generate` (inside impulses-to-states) breaks the implicit
  # pipeline -- generate ends up with no input and calls its closure in
  # single-arg mode, leaving the `s` parameter unbound.
  #
  # The reduce tracks the most-recent state record while letting non-state
  # records (signals, pulses) pass without disturbing the running answer.
  # On empty input the fold seed -- a fresh initial-state -- is returned
  # unchanged.
  impulses-to-states {
    stack: [(initial-state $game_id)]
    mode: "game"
    game_id: $game_id
    games_topic: ""
  }
  | reduce --fold (initial-state $game_id) {|item acc| $item | get state? | default $acc }
}
