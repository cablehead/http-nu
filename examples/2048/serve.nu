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
  let s0 = {tiles: [] ghosts: [] next_id: 1 score: 0 game_over: false}
  $s0 | spawn-tile | spawn-tile
}

# Drop a 2 (90%) or 4 (10%) into a random empty cell. Assigns a fresh id.
# When `dir` is given, the spawn is biased toward the trailing edge (the side
# opposite the press) so a fresh tile rarely lands between the player's
# expected merge targets. Falls back to uniform if the trailing band is empty.
def spawn-tile [dir?: string]: record -> record {
  let s = $in
  let occupied = $s.tiles | each {|t| {r: $t.r c: $t.c} }
  let all_empties = 0..3 | each {|r|
      0..3 | each {|c| {r: $r c: $c} }
    } | flatten | where {|cell| $cell not-in $occupied }
  if ($all_empties | is-empty) { return $s }
  let trailing = match $dir {
    "h" => ($all_empties | where c >= 2)
    "l" => ($all_empties | where c <= 1)
    "k" => ($all_empties | where r >= 2)
    "j" => ($all_empties | where r <= 1)
    _ => $all_empties
  }
  let empties = if ($trailing | is-empty) { $all_empties } else { $trailing }
  let pick = $empties | get (random int 0..(($empties | length) - 1))
  let value = if (random int 0..9) == 0 { 4 } else { 2 }
  $s
  | update tiles { append {id: $s.next_id r: $pick.r c: $pick.c value: $value} }
  | update next_id { $in + 1 }
}

# Slide one row left, preserving tile identity. When two adjacent tiles merge,
# the leading tile keeps its id (and doubles its value); the trailing tile is
# emitted as a ghost at the merge cell so view-transitions slide it in from
# its old position and fade it out. Returns {tiles, ghosts, score}.
def slide-row-tiles [row_idx: int]: list -> record {
  let in_row = $in | where r == $row_idx | sort-by c
  mut out = []
  mut ghosts = []
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
      $ghosts = $ghosts | append {id: $nxt.id r: $row_idx c: $col value: $nxt.value}
      $score = $score + $merged
      $col = $col + 1
      $i = $i + 2
    } else {
      $out = $out | append {id: $cur.id r: $row_idx c: $col value: $cur.value}
      $col = $col + 1
      $i = $i + 1
    }
  }
  {tiles: $out ghosts: $ghosts score: $score}
}

def slide-tiles-left []: list -> record {
  let tiles = $in
  let rows = 0..3 | each {|r| $tiles | slide-row-tiles $r }
  {
    tiles: ($rows | each {|r| $r.tiles } | flatten)
    ghosts: ($rows | each {|r| $r.ghosts } | flatten)
    score: ($rows | each {|r| $r.score } | math sum)
  }
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
      {tiles: ($r.tiles | reflect-cols) ghosts: ($r.ghosts | reflect-cols) score: $r.score}
    }
    "k" => {
      let r = $tiles | transpose-tiles | slide-tiles-left
      {tiles: ($r.tiles | transpose-tiles) ghosts: ($r.ghosts | transpose-tiles) score: $r.score}
    }
    "j" => {
      let r = $tiles | transpose-tiles | reflect-cols | slide-tiles-left
      {
        tiles: ($r.tiles | reflect-cols | transpose-tiles)
        ghosts: ($r.ghosts | reflect-cols | transpose-tiles)
        score: $r.score
      }
    }
    _ => {tiles: $tiles ghosts: [] score: 0}
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
  let s = $in | update ghosts []
  if $s.game_over { return $s }
  let r = $s.tiles | slide-tiles $dir
  if (tiles-equal $s.tiles $r.tiles) { return $s }
  let with_tile = ($s
  | update tiles { $r.tiles }
  | update ghosts { $r.ghosts }
  | update score { $in + $r.score }
  | spawn-tile $dir)
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

# Render the trailing half of a merged pair at the merge cell. Same
# view-transition-name as before, so the old pseudo (source cell, pre-merge)
# pairs with this new pseudo at the merge cell -- giving a slide. Rendered
# with opacity 0 so the new snapshot is invisible: visually the tile slides
# in and disappears under the doubled merger.
def render-ghost []: record -> record {
  let g = $in
  (DIV {
    style: {
      position: absolute
      left: $"(cell-pos $g.c)px"
      top: $"(cell-pos $g.r)px"
      width: $"($CELL)px"
      height: $"($CELL)px"
      background-color: (color-for $g.value)
      border-radius: "4px"
      view-transition-name: $"tile-($g.id)"
      opacity: 0
      "pointer-events": none
    }
  } "")
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
  let ghosts = $state.ghosts | each {|g| $g | render-ghost }
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
  } $bg $ghosts $tiles)
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
  (DIV {id: "game"} ($state | render-status) ($state | render-board))
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      let signals = $in | from datastar-signals $req
      let latency = $signals | get latency? | default 0
      if $latency > 0 {
        sleep ($latency * 1ms)
      }
      let topic = $"game.($signals.tabId).move"
      {intent: ($signals.intent? | default "")} | .bus pub $topic
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse"} {|req ctx|
      let signals = "" | from datastar-signals $req
      let tab_id = $signals | get tabId? | default "anon"
      let pattern = $"game.($tab_id).*"

      # Each SSE event carries the full game state, base64-encoded, in its
      # `id` field. The browser sends the last seen id back as
      # `Last-Event-ID` on reconnect, so we can resume mid-game without any
      # server-side persistence. Falls back to a fresh game if the header is
      # absent or unparseable.
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
        let new_state = if ($v | get init? | default false) {
          $state
        } else if $intent == "reset" {
          initial-state
        } else if $intent != "" {
          $state | apply-move $intent
        } else {
          $state
        }
        let id = $new_state | to json -r | encode base64
        {
          out: ($new_state | render-game | to datastar-patch-elements --use-view-transition --id $id)
          next: $new_state
        }
      } $init
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
      (STYLE {
        __html: "
            /* --- animation dials. tweak live in devtools to taste --- */
            :root {
              --lead-scale: 0.94;       /* how much tiles contract on press   */
              --lead-offset: 9px;       /* how far they drift in press dir    */
              --lead-duration: 1000ms;  /* breath loop while waiting          */
              --slide-duration: 280ms;  /* view-transition (tile slide) time  */
              --spawn-duration: 500ms;  /* new-tile pop duration              */
              --spawn-from: 0.2;        /* new-tile starting scale            */
              --spawn-overshoot: 1.35;  /* new-tile peak scale before settle  */
            }
            * { box-sizing: border-box; margin: 0; }
            body { display: flex; flex-direction: column; align-items: center;
                   padding: 32px; gap: 20px;
                   background: #faf8ef; color: #776e65; font-family: sans-serif; }
            aside { width: 460px; padding: 16px 20px;
                    background: #eee4da; border-radius: 8px; font-size: 14px; }
            aside hr { border: 0; border-top: 1px solid #d8cfc4; margin: 14px 0; }
            aside dl { display: grid; grid-template-columns: auto 1fr;
                       gap: 10px 14px; align-items: center; }
            aside dd { display: flex; gap: 10px; align-items: center;
                       font-variant-numeric: tabular-nums; }
            aside dd input[type=range] { flex: 1; accent-color: #f59563; }
            aside dt { font-weight: 600; }
            article { width: 460px; font-size: 15px; line-height: 1.55; }
            article p + p { margin-top: 0.8em; }
            article strong { color: #5a4f43; }
            kbd { background: #faf8ef; padding: 1px 6px; border-radius: 3px;
                  font-family: inherit; font-size: 13px; }
            #board > div:not(:empty) { transition: transform 220ms cubic-bezier(0.4, 0, 0.2, 1); }
            #board.pending > div:not(:empty) { animation: pending-breathe var(--lead-duration) ease-in-out infinite; }
            #board.pending.dir-h > div:not(:empty) { animation: lean-h var(--lead-duration) ease-in-out infinite; }
            #board.pending.dir-l > div:not(:empty) { animation: lean-l var(--lead-duration) ease-in-out infinite; }
            #board.pending.dir-k > div:not(:empty) { animation: lean-k var(--lead-duration) ease-in-out infinite; }
            #board.pending.dir-j > div:not(:empty) { animation: lean-j var(--lead-duration) ease-in-out infinite; }
            @keyframes pending-breathe {
              0%, 100% { transform: scale(var(--lead-scale)); }
              50%      { transform: scale(calc(var(--lead-scale) - 0.04)); }
            }
            @keyframes lean-h {
              0%, 100% { transform: scale(var(--lead-scale)) translateX(0); }
              50%      { transform: scale(calc(var(--lead-scale) - 0.02)) translateX(calc(-1 * var(--lead-offset))); }
            }
            @keyframes lean-l {
              0%, 100% { transform: scale(var(--lead-scale)) translateX(0); }
              50%      { transform: scale(calc(var(--lead-scale) - 0.02)) translateX(var(--lead-offset)); }
            }
            @keyframes lean-k {
              0%, 100% { transform: scale(var(--lead-scale)) translateY(0); }
              50%      { transform: scale(calc(var(--lead-scale) - 0.02)) translateY(calc(-1 * var(--lead-offset))); }
            }
            @keyframes lean-j {
              0%, 100% { transform: scale(var(--lead-scale)) translateY(0); }
              50%      { transform: scale(calc(var(--lead-scale) - 0.02)) translateY(var(--lead-offset)); }
            }
            ::view-transition-group(*) {
              animation-duration: var(--slide-duration);
              animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
            }
            /* Unpaired new pseudos -- truly new elements -- get a pop-in.
               :only-child fires when no ::view-transition-old(name) sibling
               exists, i.e. there's no old counterpart to slide from. */
            ::view-transition-new(*):only-child {
              animation: tile-spawn var(--spawn-duration) cubic-bezier(0.34, 1.56, 0.64, 1);
            }
            @keyframes tile-spawn {
              0%   { transform: scale(var(--spawn-from)); opacity: 0; }
              60%  { transform: scale(var(--spawn-overshoot)); opacity: 1; }
              100% { transform: scale(1); opacity: 1; }
            }
          "
      })
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH}))
      (BODY {
        "data-signals": "{tabId: crypto.randomUUID(), latency: 0, leadScale: 0.94, leadOffset: 9, leadDuration: 1000}"
        "data-style:--lead-scale": "$leadScale"
        "data-style:--lead-offset": "$leadOffset + 'px'"
        "data-style:--lead-duration": "$leadDuration + 'ms'"
        "data-on:keydown__window": ("
            const m = {h:'h', ArrowLeft:'h', j:'j', ArrowDown:'j', k:'k', ArrowUp:'k', l:'l', ArrowRight:'l'};
            const intent = m[evt.key] || (evt.key === 'r' ? 'reset' : '');
            if (intent) {
              const board = document.getElementById('board');
              ['dir-h','dir-j','dir-k','dir-l'].forEach(c => board.classList.remove(c));
              board.classList.add('pending');
              if (m[evt.key]) board.classList.add('dir-' + intent);
              const t0 = performance.now();
              fetch('" + ($req | href "/move") + "', {
                method: 'POST',
                headers: {'content-type': 'application/json'},
                body: JSON.stringify({tabId: $tabId, intent, latency: $latency}),
              }).then(() => {
                document.getElementById('rtt').textContent = Math.round(performance.now() - t0) + 'ms';
              });
              evt.preventDefault();
            }
          ")
      }
      (H1 "2048")
      (DIV {id: "game" "data-init": ("@get('" + ($req | href "/sse") + "')")} "")
      (ASIDE
      (P "Move with " (KBD "h j k l") " or arrows. " (KBD "r") " resets.")
      (HR)
      (DL
      (DT "Latency")
      (DD
      (INPUT {type: "range" min: "0" max: "1000" step: "10" value: "0" "data-bind:latency": true})
      (OUTPUT {"data-text": "$latency + 'ms'"} "0ms"))
      (DT "RTT")
      (DD (OUTPUT {id: "rtt"} "0ms"))
      (DT "Contract")
      (DD
      (INPUT {type: "range" min: "0.5" max: "1" step: "0.01" value: "0.94" "data-bind:lead-scale": true})
      (OUTPUT {"data-text": "$leadScale"} "0.94"))
      (DT "Drift")
      (DD
      (INPUT {type: "range" min: "0" max: "30" step: "1" value: "9" "data-bind:lead-offset": true})
      (OUTPUT {"data-text": "$leadOffset + 'px'"} "9px"))
      (DT "Breath")
      (DD
      (INPUT {type: "range" min: "200" max: "2000" step: "50" value: "1000" "data-bind:lead-duration": true})
      (OUTPUT {"data-text": "$leadDuration + 'ms'"} "1000ms"))))
      (ARTICLE
      (P "Each keypress fires a fetch to http-nu, which publishes through "
      "an in-process pub/sub bus; a long-lived SSE connection picks the "
      "message up and patches the board.")
      (P (STRONG "Latency") " adds server-side sleep; "
      (STRONG "RTT") " is the round-trip you actually experienced.")
      (P "While the wait runs, tiles squeeze and lean toward the press "
      "direction. "
      (STRONG "Contract") " sets the squeeze depth, "
      (STRONG "Drift") " how far they lean, "
      (STRONG "Breath") " the cycle period."))))
    })
  ]
}
