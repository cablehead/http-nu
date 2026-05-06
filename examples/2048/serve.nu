use http-nu/router *
use http-nu/datastar *
use http-nu/html *

const SCRIPT_DIR = path self | path dirname

# 2048 over the Local Bus, with View Transition tile slides.
#
# State is a list of tiles `{id, r, c, value}` -- not a flat grid -- so each
# tile keeps a stable identity across moves. Render emits each tile as an
# absolutely-positioned div with `view-transition-name: tile-<id>`. When the
# board re-renders, the browser pairs old and new snapshots by name and
# animates the position interpolation for free, giving the classic "tiles
# slide in the direction of motion" feel.

# --- game logic -----------------------------------------------------------

def initial-state []: nothing -> record {
  let s0 = {tiles: [] next_id: 1 score: 0 game_over: false}
  $s0 | spawn-tile | spawn-tile
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
  {tiles: ($rows | each {|r| $r.tiles } | flatten) score: ($rows | each {|r| $r.score } | math sum)}
}

# Reflect tiles over the vertical axis (c -> 3 - c).
def reflect-cols []: list -> list {
  $in | each {|t| $t | upsert c (3 - $t.c) }
}

# Swap r and c (transpose tiles over the diagonal).
def transpose-tiles []: list -> list {
  $in | each {|t| $t | upsert r $t.c | upsert c $t.r }
}

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
      let right_match = $t.c < 3 and ($s.tiles | where r == $t.r and c == ($t.c + 1) | first | get value) == $t.value
      let down_match = $t.r < 3 and ($s.tiles | where r == ($t.r + 1) and c == $t.c | first | get value) == $t.value
      $right_match or $down_match
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

const CELL = 80
const GAP = 8
const PAD = 12
# inner = 4*CELL + 3*GAP = 344, total = inner + 2*PAD = 368
const TOTAL = 368

def cell-pos [n: int]: nothing -> int {
  $PAD + $n * ($CELL + $GAP)
}

def render-tile []: record -> record {
  let t = $in
  (DIV {
    style: {
      position: absolute
      left: $"(cell-pos $t.c)px"
      top: $"(cell-pos $t.r)px"
      width: $"($CELL)px"
      height: $"($CELL)px"
      display: flex
      align-items: center
      justify-content: center
      background-color: (color-for $t.value)
      color: (if $t.value <= 4 { "#776e65" } else { "#f9f6f2" })
      font-size: (if $t.value >= 1024 { "24px" } else if $t.value >= 128 { "28px" } else { "32px" })
      font-weight: "bold"
      border-radius: "4px"
      view-transition-name: $"tile-($t.id)"
    }
  } ($t.value | into string))
}

def render-empty-cell [r: int c: int]: nothing -> record {
  (DIV {
    style: {
      position: absolute
      left: $"(cell-pos $c)px"
      top: $"(cell-pos $r)px"
      width: $"($CELL)px"
      height: $"($CELL)px"
      background: "#cdc1b4"
      border-radius: "4px"
    }
  } "")
}

def render-board []: record -> record {
  let state = $in
  let bg = 0..3 | each {|r| 0..3 | each {|c| render-empty-cell $r $c } } | flatten
  let tiles = $state.tiles | each {|t| $t | render-tile }
  (DIV {
    id: "board"
    style: {
      position: relative
      width: $"($TOTAL)px"
      height: $"($TOTAL)px"
      background: "#bbada0"
      border-radius: "6px"
    }
  } $bg $tiles)
}

def render-status []: record -> record {
  let state = $in
  let tail = if $state.game_over {
    [(SPAN {style: {color: "#c0392b" margin-left: "16px"}} "GAME OVER (press r)")]
  } else { [] }
  (DIV {
    id: "status"
    style: {font-family: "monospace" font-size: "18px" margin-bottom: "12px"}
  } ([(SPAN $"Score: ($state.score)")] | append $tail))
}

def render-game []: record -> record {
  let state = $in
  (DIV {id: "game"} ($state | render-status) ($state | render-board))
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      let signals = $in | from datastar-signals $req
      let topic = $"game.($signals.tabId).move"
      $signals | reject tabId | .bus pub $topic
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse"} {|req ctx|
      let signals = "" | from datastar-signals $req
      let tab_id = $signals | get tabId? | default "anon"
      let pattern = $"game.($tab_id).*"

      .bus sub $pattern
      | prepend {topic: "init" value: {init: true}}
      | generate {|impulse state|
        let v = $impulse.value
        let intent = $v | get intent? | default ""
        let new_state = if ($v | get init? | default false) {
          $state
        } else if $intent == "reset" {
          initial-state
        } else if $intent != "" {
          $state | apply-move $intent
        } else {
          $state
        }
        {out: ($new_state | render-game | to datastar-patch-elements --use-view-transition) next: $new_state}
      } (initial-state)
      | to sse
    })

    (route {method: GET path: "/og.png"} {|req ctx|
      .static $SCRIPT_DIR "/og.png"
    })

    (route {method: GET path: "/"} {|req ctx|
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      (HTML
      (HEAD
      (META {charset: "utf-8"})
      (LINK {rel: "icon" href: "data:,"})
      (TITLE "2048 -- http-nu .bus demo")
      (META {property: "og:type" content: "website"})
      (META {property: "og:title" content: "2048 over the http-nu Local Bus"})
      (META {
        property: "og:description"
        content: "Solo-tab 2048 driven by .bus pub/sub with view-transition tile slides."
      })
      (META {property: "og:image" content: $og_image})
      (META {name: "twitter:card" content: "summary_large_image"})
      (META {name: "twitter:image" content: $og_image})
      (STYLE "
            * { box-sizing: border-box; margin: 0; }
            body { display: flex; flex-direction: column; align-items: center;
                   padding: 32px; background: #faf8ef; font-family: sans-serif; }
            h1 { color: #776e65; margin-bottom: 12px; }
            .hint { color: #776e65; margin-top: 16px; font-size: 14px; }
            .hint code { background: #eee4da; padding: 1px 6px; border-radius: 3px; }
            ::view-transition-group(*) {
              animation-duration: 150ms;
              animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
            }
          ")
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH}))
      (BODY {
        "data-signals": "{tabId: crypto.randomUUID(), intent: ''}"
        "data-on:keydown__window": ("
            const m = {h:'h', ArrowLeft:'h', j:'j', ArrowDown:'j', k:'k', ArrowUp:'k', l:'l', ArrowRight:'l'};
            $intent = m[evt.key] || (evt.key === 'r' ? 'reset' : '');
            if ($intent) { @post('" + ($req | href "/move") + "'); evt.preventDefault(); }
          ")
      }
      (H1 "2048")
      (DIV {id: "game" "data-init": ("@get('" + ($req | href "/sse") + "')")} "")
      (DIV {class: "hint"}
      "Move with " (CODE "h j k l") " or arrow keys. "
      (CODE "r") " resets.")))
    })
  ]
}
