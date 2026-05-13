use http-nu/router *
use http-nu/datastar *
use http-nu/html *

const SCRIPT_DIR = path self | path dirname
const STATIC_DIR = $SCRIPT_DIR | path join "static"
# Cache-buster for static assets: fresh per server start, stable within one
# session. Browsers cache /styles.css?v=<REV> across page loads but refetch
# on the next server restart.
let REV = random uuid | str substring 0..7

# 2048 over the Local Bus, with View Transition tile slides.
#
# State is a list of tiles `{id, r, c, value}` -- not a flat grid -- so each
# tile keeps a stable identity across moves. Render emits each tile as an
# absolutely-positioned div with `view-transition-name: tile-<id>`. When the
# board re-renders, the browser pairs old and new snapshots by name and
# animates the position interpolation for free.

# --- game logic -----------------------------------------------------------

def initial-state []: nothing -> record {
  let s1 = fresh-spawn-seeds
  let s2 = fresh-spawn-seeds
  {tiles: [] next_id: 1 score: 0 game_over: false}
  | spawn-tile $s1.idx $s1.value
  | spawn-tile $s2.idx $s2.value
}

# Drop a 2 (90%) or 4 (10%) into an empty cell, picking via the supplied
# seeds. Seeds are generated at POST time and recorded in the move frame so
# that replay reproduces the exact same board.
def spawn-tile [idx_seed: int, value_seed: int]: record -> record {
  let s = $in
  let occupied = $s.tiles | each {|t| {r: $t.r c: $t.c} }
  let empties = 0..3 | each {|r|
      0..3 | each {|c| {r: $r c: $c} }
    } | flatten | where {|cell| $cell not-in $occupied }
  if ($empties | is-empty) { return $s }
  let pick = $empties | get ($idx_seed mod ($empties | length))
  let value = if ($value_seed == 0) { 4 } else { 2 }
  $s
  | update tiles { append {id: $s.next_id r: $pick.r c: $pick.c value: $value} }
  | update next_id { $in + 1 }
}

# Helper for callers that just want fresh randomness (route /, /move).
def fresh-spawn-seeds []: nothing -> record {
  {idx: (random int 0..999999), value: (random int 0..9)}
}

# Slide one row left, preserving tile identity. When two adjacent tiles merge,
# the leading tile keeps its id (and doubles its value); the trailing tile is
# removed. Returns {tiles, score}.
def slide-row-tiles [row_idx: int]: list -> record {
  let in_row = $in | where r == $row_idx | sort-by c
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

def slide-tiles-left []: list -> record {
  let tiles = $in
  let rows = 0..3 | each {|r| $tiles | slide-row-tiles $r }
  {
    tiles: ($rows | each {|r| $r.tiles } | flatten)
    score: ($rows | each {|r| $r.score } | math sum)
  }
}

# Reflect tiles over the vertical axis (c -> 3 - c).
def reflect-cols []: list -> list {
  $in | each {|t| $t | upsert c (3 - $t.c) }
}

# Swap r and c (transpose over the diagonal).
def transpose-tiles []: list -> list {
  $in | each {|t| $t | upsert r $t.c | upsert c $t.r }
}

# All four directions reuse `slide-tiles-left` by reflecting/transposing in
# and back out -- so the merge logic lives in exactly one place.
def slide-tiles [dir: string]: list -> record {
  let tiles = $in
  match $dir {
    "h" => ($tiles | slide-tiles-left)
    "l" => {
      let r = $tiles | reflect-cols | slide-tiles-left
      {tiles: ($r.tiles | reflect-cols) score: $r.score}
    }
    "k" => {
      let r = $tiles | transpose-tiles | slide-tiles-left
      {tiles: ($r.tiles | transpose-tiles) score: $r.score}
    }
    "j" => {
      let r = $tiles | transpose-tiles | reflect-cols | slide-tiles-left
      {tiles: ($r.tiles | reflect-cols | transpose-tiles) score: $r.score}
    }
    _ => {tiles: $tiles score: 0}
  }
}

def tiles-equal [a: list b: list]: nothing -> bool {
  ($a | sort-by id) == ($b | sort-by id)
}

def is-game-over []: record -> bool {
  let s = $in
  if ($s.tiles | length) < 16 { return false }
  let mergeable = $s.tiles | any {|t|
      let right = $t.c < 3 and ($s.tiles | where r == $t.r and c == ($t.c + 1) | first | get value) == $t.value
      let down = $t.r < 3 and ($s.tiles | where r == ($t.r + 1) and c == $t.c | first | get value) == $t.value
      $right or $down
    }
  not $mergeable
}

def apply-move [dir: string, idx_seed: int, value_seed: int]: record -> record {
  let s = $in
  if $s.game_over { return $s }
  let r = $s.tiles | slide-tiles $dir
  if (tiles-equal $s.tiles $r.tiles) { return $s }
  let with_tile = ($s
  | update tiles { $r.tiles }
  | update score { $in + $r.score }
  | spawn-tile $idx_seed $value_seed)
  $with_tile | update game_over { $with_tile | is-game-over }
}

# --- rendering ------------------------------------------------------------

def color-for [v: int]: nothing -> string {
  match $v {
    2 => "#eee4da"
    4 => "#ede0c8"
    8 => "#f2b179"
    16 => "#f59563"
    32 => "#f67c5f"
    64 => "#f65e3b"
    128 => "#edcf72"
    256 => "#edcc61"
    512 => "#edc850"
    1024 => "#edc53f"
    _ => "#edc22e"
  }
}

const CELL = 100
const GAP = 10
const PAD = 15
# inner = 4*CELL + 3*GAP = 430, total = inner + 2*PAD = 460
const TOTAL = 460

def render-tile []: record -> record {
  let t = $in
  # Grid placement: column/row indices are 1-based in CSS Grid.
  (DIV {class: "tile" style: {
    grid-column: ($t.c + 1 | into string)
    grid-row: ($t.r + 1 | into string)
    display: flex  align-items: center  justify-content: center
    background-color: (color-for $t.value)
    color: (if $t.value <= 4 { "#776e65" } else { "#f9f6f2" })
    font-size: (if $t.value >= 1024 { "24px" } else if $t.value >= 128 { "28px" } else { "32px" })
    font-weight: "bold"  border-radius: "4px"
    view-transition-name: $"tile-($t.id)"
  }} ($t.value | into string))
}

def render-empty-cell [r: int c: int]: nothing -> record {
  (DIV {style: {
    grid-column: ($c + 1 | into string)
    grid-row: ($r + 1 | into string)
    background: "#cdc1b4"  border-radius: "4px"
  }} "")
}

def render-board []: record -> record {
  let state = $in
  let bg = 0..3 | each {|r| 0..3 | each {|c| render-empty-cell $r $c } } | flatten
  let tiles = $state.tiles | each {|t| $t | render-tile }
  # 4x4 grid; cells and tiles share placement via grid-column / grid-row.
  (DIV {id: "board" style: {
    display: grid
    grid-template-columns: $"repeat\(4, ($CELL)px\)"
    grid-template-rows: $"repeat\(4, ($CELL)px\)"
    gap: $"($GAP)px"
    padding: $"($PAD)px"
    width: $"($TOTAL)px"  height: $"($TOTAL)px"
    background: "#bbada0"  border-radius: "6px"
  }} $bg $tiles)
}

def render-status []: record -> record {
  let state = $in
  let won = $state.tiles | any {|t| $t.value >= 2048 }
  let tail = if $state.game_over {
    [(SPAN {style: {color: "#c0392b" margin-left: "16px"}} "GAME OVER (press r)")]
  } else if $won {
    [(SPAN {style: {color: "#388e3c" margin-left: "16px"}} "YOU WIN! (keep going or press r)")]
  } else { [] }
  (DIV {
    id: "status"
    style: {font-family: "monospace" font-size: "18px" margin-bottom: "12px"}
  } ([(SPAN $"Score: ($state.score)")] | append $tail))
}

def gear-button []: nothing -> record {
  (BUTTON {class: "settings-toggle" type: "button" aria-label: "settings" "data-view-to": "settings"}
    (ICONIFY "material-symbols:settings-outline-rounded" {width: "20" height: "20"}))
}

def close-button []: nothing -> record {
  (BUTTON {class: "settings-toggle" type: "button" aria-label: "close" "data-view-to": "game"}
    (ICONIFY "material-symbols:close-rounded" {width: "20" height: "20"}))
}

def render-game [direction?: string, changed?: bool]: record -> record {
  let state = $in
  # The edge-glow color rides the highest-value tile, pushed as an inline
  # CSS variable so it cascades to #board-wrap and the ::after pseudo.
  let glow = color-for (if ($state.tiles | is-empty) { 2 } else { $state.tiles | get value | math max })
  let dir = $direction | default ""
  let did_change = $changed | default false
  # A fresh edge-flash element per patch (unique id forces morphdom to
  # destroy/recreate, so its CSS animation re-fires every step -- including
  # each step of a shift sequence). When the move did not change the board
  # (slide had no effect), skip the flash.
  let wrap_children = if ($did_change and $dir in [h j k l]) {
    [
      ($state | render-board)
      (DIV {id: $"flash-(random uuid)" class: "edge-flash" "data-dir": $dir} "")
    ]
  } else {
    [($state | render-board)]
  }
  # data-rev forces a unique attribute per render so datastar's morph always
  # touches something -- otherwise no-op patches (e.g. ping echoes) wouldn't
  # fire a MutationObserver event and the RTT readout would never seed.
  # data-view tells the client which mode the current render is in.
  # data-changed signals to the client whether the board state actually
  # moved (vs a no-op direction press); used to gate haptics + edge flash.
  # view-transition-name: per-mode so a game<->settings switch becomes an
  # UNPAIRED pseudo (game->game stays paired and cross-fades as usual).
  (DIV {
    id: "game"
    style: $"--glow: ($glow); view-transition-name: view-game;"
    "data-rev": (random uuid)
    "data-view": "game"
    "data-from": $dir
    "data-changed": (if $did_change { "1" } else { "" })
  }
    (gear-button)
    ($state | render-status)
    (DIV {id: "board-wrap"} ...$wrap_children))
}

def render-settings []: nothing -> record {
  (DIV {
    id: "game"
    style: "view-transition-name: view-settings;"
    "data-rev": (random uuid)
    "data-view": "settings"
  }
    (close-button)
    (DIV {id: "settings-panel"}
      (H2 "settings")
      (P "more knobs soon.")))
}

# Pick the right render based on the per-tab mode. Same #game id either way
# so datastar morphs the swap as a single replacement.
def render-current [mode: string, direction?: string, changed?: bool]: record -> record {
  let state = $in
  if $mode == "settings" {
    render-settings
  } else {
    $state | render-game $direction $changed
  }
}

# --- pipeline boxes ------------------------------------------------------
# The SSE handler is a tight composition of:
#   .cat --follow -> impulses-to-states -> threshold-gate -> states-to-patches -> to sse
# Each stage has one job.

# Box A. Takes xs frames, yields {state, mode, threshold?} records. A normal
# move yields one record; a shift-intent runs apply-move until the board
# settles and yields one record per successful step, paced 50ms apart so the
# client sees each step animate. Pacing lives here because this is the box
# that knows the difference between a one-shot move and a multi-step shift.
def impulses-to-states [initial: record] {
  generate {|frame s|
    if $frame.topic == "xs.threshold" {
      return {out: [{state: $s.state, mode: $s.mode, threshold: true}], next: $s}
    }
    let kind = $frame.meta | get kind? | default "move"
    if $kind == "start" {
      let new_state = $frame.meta.state
      return {out: [{state: $new_state, mode: $s.mode, threshold: false}], next: ($s | upsert state $new_state)}
    }
    if $kind == "view" {
      # Ephemeral (ttl=ephemeral): only arrives live, never during replay,
      # so mode resets to game on reconnect.
      let new_s = $s | upsert mode ($frame.meta | get mode? | default "game")
      return {out: [{state: $new_s.state, mode: $new_s.mode, threshold: false}], next: $new_s}
    }
    let intent = $frame.meta | get intent? | default ""
    if $intent in [h j k l] {
      let idx = $frame.meta | get spawn_idx? | default 0
      let val = $frame.meta | get spawn_value? | default 0
      let new_state = $s.state | apply-move $intent $idx $val
      let changed = not (tiles-equal $s.state.tiles $new_state.tiles)
      return {out: [{state: $new_state, mode: $s.mode, direction: $intent, changed: $changed, threshold: false}], next: ($s | upsert state $new_state)}
    }
    if ($intent | str starts-with "shift-") {
      let dir = $intent | str substring 6..
      let seeds = $frame.meta | get seeds? | default []
      let result = $seeds | reduce --fold {state: $s.state, stopped: false, yields: []} {|seed acc|
        if $acc.stopped { return $acc }
        let nxt = $acc.state | apply-move $dir $seed.idx $seed.value
        if (tiles-equal $acc.state.tiles $nxt.tiles) {
          $acc | upsert stopped true
        } else {
          $acc | update state $nxt | update yields {
            append {state: $nxt, mode: $s.mode, direction: $dir, changed: true, threshold: false, paced: true}
          }
        }
      }
      # If the shift was a complete no-op (first step didn't move), still
      # emit a state echo so the client sees a mutation and clears pending.
      if ($result.yields | is-empty) {
        return {out: [{state: $s.state, mode: $s.mode, threshold: false}], next: $s}
      }
      return {out: $result.yields, next: ($s | upsert state $result.state)}
    }
    # Any other intent (empty ping, unrecognised) still emits a no-op state
    # echo. The client uses the resulting mutation to measure RTT and to
    # clear the "lit edge" -- if the edge stays lit, the server is slow.
    return {out: [{state: $s.state, mode: $s.mode, threshold: false}], next: $s}
  } $initial
  | flatten
  | generate {|item state = {prev_paced: false}|
    # Pace consecutive paced items 100ms apart so each shift step animates.
    if (($item.paced? | default false) and $state.prev_paced) { sleep 175ms }
    {out: $item, next: {prev_paced: ($item.paced? | default false)}}
  }
}

# Box B. Buffers states pre-threshold (only the last is retained); on
# threshold marker emits the last buffered state; then forwards everything.
# Same pattern as examples/quotes/serve.nu's threshold-once-gate, but
# operating on state records instead of raw xs frames.
def threshold-gate-states [] {
  generate {|item state = {}|
    if ($item.threshold? | default false) {
      return {out: $state.last?, next: {reached: true}}
    }
    if ("reached" in $state) {
      return {out: $item, next: $state}
    }
    {next: ($state | upsert last $item)}
  }
}

# Box C. Pure rendering. State -> datastar patch event.
def states-to-patches [] {
  each {|s|
    $s.state | render-current $s.mode ($s.direction? | default "") ($s.changed? | default false)
    | to datastar-patch-elements --use-view-transition --id (random uuid)
  }
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      # One frame per request. The state machine in /sse handles whatever
      # the intent demands -- shift-intents replay deterministically from
      # the bundled seeds.
      let signals = $in | from datastar-signals $req
      let topic = $"game.($signals.tabId).move"
      let intent = $signals | get intent? | default ""
      if $intent == "reset" {
        null | .append $topic --meta {kind: "start" state: (initial-state)}
      } else if ($intent | str starts-with "shift-") {
        let seeds = 0..15 | each {|_| fresh-spawn-seeds}
        null | .append $topic --meta {intent: $intent seeds: $seeds}
      } else {
        let seeds = fresh-spawn-seeds
        null | .append $topic --meta {
          intent: $intent
          spawn_idx: $seeds.idx
          spawn_value: $seeds.value
        }
      }
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse"} {|req ctx|
      let signals = "" | from datastar-signals $req
      let tab_id = $signals | get tabId? | default "anon"
      let topic = $"game.($tab_id).move"

      .cat --follow
      | where {|f| $f.topic == $topic or $f.topic == "xs.threshold"}
      | impulses-to-states {state: (initial-state), mode: "game"}
      | threshold-gate-states
      | states-to-patches
      | to sse
    })

    (route {method: GET path: "/script.js"} {|req ctx|
      .static $STATIC_DIR "/script.js"
    })

    (route {method: GET path: "/styles.css"} {|req ctx|
      .static $STATIC_DIR "/styles.css"
    })

    (route {method: GET path: "/ellie.png"} {|req ctx|
      .static $STATIC_DIR "/ellie.png"
    })

    (route {method: GET path: "/og.png"} {|req ctx|
      .static $SCRIPT_DIR "/og.png"
    })

    (route {method: POST path: "/view"} {|req ctx|
      # View changes go on the same per-tab topic as moves, but with
      # ttl=ephemeral so they are NOT persisted -- only currently-connected
      # subscribers receive them. Reconnecting always starts in game mode.
      let signals = $in | from datastar-signals $req
      let topic = $"game.($signals.tabId).move"
      let mode = $signals | get mode? | default "game"
      null | .append $topic --ttl ephemeral --meta {kind: "view" mode: $mode}
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/"} {|req ctx|
      let tab_id = random uuid
      # Seed a "start" frame: the canonical initial state for this tab. Every
      # subsequent /sse replays this plus the move log to recompute state, so
      # the random tile placements are captured once and reproduced faithfully.
      let initial = initial-state
      null | .append $"game.($tab_id).move" --meta {kind: "start" state: $initial}
      # Render an EMPTY board as the placeholder: same dimensions (grid cells
      # fill it) so no layout jump, but no tiles in the DOM yet. When the SSE
      # init patch arrives with the real tiles, they're unpaired (:only-child)
      # which fires the spawn pop-in -- otherwise paired tiles would just
      # cross-fade with no animation.
      let placeholder = $initial | upsert tiles [] | render-game
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      (HTML
      (HEAD
      (META {charset: "utf-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no"})
      (LINK {rel: "icon" href: "data:,"})
      (TITLE "2048 -- http-nu .bus demo")
      (META {property: "og:type" content: "website"})
      (META {property: "og:title" content: "2048.nu"})
      (META {property: "og:description" content: "Solo-tab 2048 driven by .bus pub/sub with view-transition tile slides."})
      (META {property: "og:image" content: $og_image})
      (META {name: "twitter:card" content: "summary_large_image"})
      (META {name: "twitter:image" content: $og_image})
      (LINK {rel: "stylesheet" href: ($req | href $"/styles.css?v=($REV)")})
      (SCRIPT-ICONIFY)
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
      (SCRIPT {src: ($req | href $"/script.js?v=($REV)") defer: true}))
      (BODY {
        # tabId is generated server-side per page load so datastar's @get URL
        # and the input handlers in script.js share one id.
        "data-tab-id": $tab_id
        "data-move-url": ($req | href "/move")
        "data-view-url": ($req | href "/view")
        "data-signals": $"{tabId: '($tab_id)'}"
        # Mirror datastar's $connected signal (set by data-indicator on #game)
        # into a data-attr CSS can react to.
        "data-attr:data-conn": "$connected ? 'ok' : 'down'"
      }
      (H1 (A {
        href: "https://github.com/cablehead/http-nu/blob/main/examples/2048/serve.nu"
      } "2048.nu"))
      (P {class: "hint"}
        "Letter or arrow keys "
        (KBD "h \u{2190}") " "
        (KBD "j \u{2193}") " "
        (KBD "k \u{2191}") " "
        (KBD "l \u{2192}")
        ", or swipe. Reset: "
        (BUTTON {type: "button"} "r"))
      # data-init and data-indicator live on .column (which is never patched)
      # so the SSE fetch + connection signal survive the wholesale replacement
      # of #game's contents on every server patch.
      (DIV {
        class: "column"
        # data-indicator MUST come before data-init so the signal exists when
        # the fetch fires.
        "data-indicator": "connected"
        "data-init": ("@get('" + ($req | href "/sse") + "', {retry: 'always'})")
      }
        # #game is the single view; SSE patches morph it between the game
        # board render and the settings panel render based on per-tab mode
        # in the event log.
        $placeholder
        (FOOTER
          (SPAN {class: "status"}
            (SPAN {id: "conn" title: "SSE connection"})
            (SPAN {id: "rtt"} "\u{2014}ms"))
          (SPAN {class: "credit"}
            (A {href: "https://http-nu.cross.stream"}
              "served by http-nu "
              (IMG {src: ($req | href "/ellie.png") alt: "ellie" class: "mascot"})))))))
    })
  ]
}
