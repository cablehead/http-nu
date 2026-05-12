use http-nu/router *
use http-nu/datastar *
use http-nu/html *

const STATIC_DIR = path self | path dirname | path join "static"

# 2048 over the Local Bus, with View Transition tile slides.
#
# State is a list of tiles `{id, r, c, value}` -- not a flat grid -- so each
# tile keeps a stable identity across moves. Render emits each tile as an
# absolutely-positioned div with `view-transition-name: tile-<id>`. When the
# board re-renders, the browser pairs old and new snapshots by name and
# animates the position interpolation for free.

# --- game logic -----------------------------------------------------------

def initial-state []: nothing -> record {
  {tiles: [] next_id: 1 score: 0 game_over: false}
  | spawn-tile | spawn-tile
}

# Drop a 2 (90%) or 4 (10%) into a random empty cell. Assigns a fresh id.
def spawn-tile []: record -> record {
  let s = $in
  let occupied = $s.tiles | each {|t| {r: $t.r c: $t.c} }
  let empties = 0..3 | each {|r|
      0..3 | each {|c| {r: $r c: $c} }
    } | flatten | where {|cell| $cell not-in $occupied }
  if ($empties | is-empty) { return $s }
  let pick = $empties | get (random int 0..(($empties | length) - 1))
  let value = if (random int 0..9) == 0 { 4 } else { 2 }
  $s
  | update tiles { append {id: $s.next_id r: $pick.r c: $pick.c value: $value} }
  | update next_id { $in + 1 }
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

def apply-move [dir: string]: record -> record {
  let s = $in
  if $s.game_over { return $s }
  let r = $s.tiles | slide-tiles $dir
  if (tiles-equal $s.tiles $r.tiles) { return $s }
  let with_tile = ($s
  | update tiles { $r.tiles }
  | update score { $in + $r.score }
  | spawn-tile)
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

def cell-pos [n: int]: nothing -> int { $PAD + $n * ($CELL + $GAP) }

def render-tile []: record -> record {
  let t = $in
  (DIV {class: "tile" style: {
    position: absolute  left: $"(cell-pos $t.c)px"  top: $"(cell-pos $t.r)px"
    width: $"($CELL)px"  height: $"($CELL)px"
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
    position: absolute  left: $"(cell-pos $c)px"  top: $"(cell-pos $r)px"
    width: $"($CELL)px"  height: $"($CELL)px"
    background: "#cdc1b4"  border-radius: "4px"
  }} "")
}

def render-board []: record -> record {
  let state = $in
  let bg = 0..3 | each {|r| 0..3 | each {|c| render-empty-cell $r $c } } | flatten
  let tiles = $state.tiles | each {|t| $t | render-tile }
  (DIV {id: "board" style: {
    position: relative  width: $"($TOTAL)px"  height: $"($TOTAL)px"
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

def render-game []: record -> record {
  let state = $in
  # The edge-glow color rides the highest-value tile, pushed as an inline
  # CSS variable so it cascades to #board-wrap and the ::after pseudo.
  let glow = color-for ($state.tiles | get value | math max)
  # board sits inside a scaled wrap so the page can fit any viewport without
  # touching the board's internal 460px coordinate system.
  (DIV {id: "game" style: $"--glow: ($glow);"}
    ($state | render-status)
    (DIV {id: "board-wrap"} ($state | render-board)))
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      let signals = $in | from datastar-signals $req
      let topic = $"game.($signals.tabId).move"
      {intent: ($signals.intent? | default "")} | .bus pub $topic
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse"} {|req ctx|
      let signals = "" | from datastar-signals $req
      let tab_id = $signals | get tabId? | default "anon"
      let pattern = $"game.($tab_id).*"

      # Each SSE event carries the full game state, base64-encoded, in its
      # `id` field. On reconnect the browser sends the last id back as
      # `Last-Event-ID`, so we resume mid-game with no server-side
      # persistence. Falls back to a fresh game if absent or unparseable.
      let init = try {
        $req.headers | get last-event-id | decode base64 | decode utf-8 | from json
      } catch {
        initial-state
      }

      .bus sub $pattern
      | prepend {topic: "init" value: {init: true}}
      | generate {|impulse state|
        let v = $impulse.value
        let intent = $v | get intent? | default ""
        let new_state = if ($v | get init? | default false) { $state
        } else if $intent == "reset" { initial-state
        } else if $intent != "" { $state | apply-move $intent
        } else { $state }
        let id = $new_state | to json -r | encode base64
        {
          out: ($new_state | render-game | to datastar-patch-elements --use-view-transition --id $id)
          next: $new_state
        }
      } $init
      | to sse
    })

    (route {method: GET path: "/script.js"} {|req ctx|
      .static $STATIC_DIR "/script.js"
    })

    (route {method: GET path: "/styles.css"} {|req ctx|
      .static $STATIC_DIR "/styles.css"
    })

    (route {method: GET path: "/"} {|req ctx|
      let tab_id = random uuid
      (HTML
      (HEAD
      (META {charset: "utf-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no"})
      (LINK {rel: "icon" href: "data:,"})
      (TITLE "2048 -- http-nu .bus demo")
      (LINK {rel: "stylesheet" href: ($req | href "/styles.css")})
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
      (SCRIPT {src: ($req | href "/script.js") defer: true}))
      (BODY {
        # tabId is generated server-side per page load so datastar's @get URL
        # and the input handlers in script.js share one id.
        "data-tab-id": $tab_id
        "data-move-url": ($req | href "/move")
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
        (DIV {id: "game"} "")
        (FOOTER
          (SPAN {id: "conn" title: "SSE connection"})
          (SPAN {id: "rtt"} "0ms") " "
          "served by " (A {href: "https://http-nu.cross.stream"} "http-nu")))))
    })
  ]
}
