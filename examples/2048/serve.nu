use http-nu/router *
use http-nu/datastar *
use http-nu/html *
use http-nu/http *

# Pure game logic + replay helpers live in mod.nu so they're reusable from
# `http-nu eval` for ad-hoc poking at a game store.
use ./mod.nu *

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

def render-game [direction?: string, changed?: bool, req_id?: string]: record -> record {
  let state = $in
  # The edge-glow color rides the highest-value tile, pushed as an inline
  # CSS variable so it cascades to #board-wrap and the ::after pseudo.
  let glow = color-for (if ($state.tiles | is-empty) { 2 } else { $state.tiles | get value | math max })
  let dir = $direction | default ""
  let did_change = $changed | default false
  let rid = $req_id | default ""
  # A fresh edge-flash element per patch (unique id forces morphdom to
  # destroy/recreate, so its CSS animation re-fires every step -- including
  # each step of a slam cascade). When the move did not change the board
  # (slide had no effect), skip the flash.
  let wrap_children = if ($did_change and $dir in [h j k l]) {
    [
      ($state | render-board)
      (DIV {id: $"flash-(random uuid)" class: "edge-flash" "data-dir": $dir} "")
    ]
  } else {
    [($state | render-board)]
  }
  # data-rev = client's reqId when the patch is the response to a specific
  # move POST; otherwise a fresh uuid. The client's RTT observer only counts
  # a mutation when data-rev matches its pending probe id.
  # data-view tells the client which mode the current render is in.
  # data-changed signals to the client whether the board state actually
  # moved (vs a no-op direction press); used to gate haptics + edge flash.
  # view-transition-name: per-mode so a game<->settings switch becomes an
  # UNPAIRED pseudo (game->game stays paired and cross-fades as usual).
  (DIV {
    id: "game"
    style: $"--glow: ($glow); view-transition-name: view-game;"
    "data-rev": (if ($rid | is-empty) { random uuid } else { $rid })
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

# One row in the past-games list: thumbnail + score + max-tile + move count.
def render-game-card [req: record game_frame: record]: nothing -> record {
  let game_id = $game_frame.id
  let resumed = (resume-game $game_id)
  let state = $resumed.state
  let max_tile = if ($state.tiles | is-empty) { 0 } else {
    $state.tiles | get value | math max
  }
  let move_count = $resumed.moves
  (DIV {class: "game-card"}
    (DIV {class: "thumb"} ($state | render-board))
    (DIV {class: "meta"}
      (DIV {class: "score"} $"Score: ($state.score)")
      (DIV {} $"Max tile: ($max_tile)")
      (DIV {} $"Moves: ($move_count)")
      (if $state.game_over {
        (DIV {class: "badge over"} "game over")
      } else {
        (DIV {class: "badge live"} "in progress")
      })))
}

# Pick the right render based on the per-tab mode. Same #game id either way
# so datastar morphs the swap as a single replacement.
def render-current [mode: string, direction?: string, changed?: bool, req_id?: string]: record -> record {
  let state = $in
  if $mode == "settings" {
    render-settings
  } else {
    $state | render-game $direction $changed $req_id
  }
}

# --- pipeline boxes ------------------------------------------------------
# The SSE handler is a tight composition of:
#   .cat --follow -> filter-for-player -> impulses-to-states -> pace-slam-steps
#   -> threshold-gate-states -> states-to-html -> html-to-patches -> to sse
# Each stage has one job.

# Box A. impulses-to-states (and filter-for-player) live in mod.nu so they're
# reusable from `http-nu eval`. The remaining boxes here are SSE-specific:
# pacing, threshold gating, render, and patch wrapping.

# Pace consecutive paced items 200ms apart so each slam step animates over
# the wire. Separated from impulses-to-states so replay paths (e.g. /games
# rendering each past game's final state) can skip the wait.
def pace-slam-steps [] {
  generate {|item state = {prev_paced: false}|
    if (($item.paced? | default false) and $state.prev_paced) { sleep 200ms }
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
      # Empty log: nothing buffered. Emit the threshold marker's own state
      # (the placeholder top-of-stack from impulses-to-states) so downstream
      # always sees a non-null record. Strip the threshold flag.
      let emit = $state.last? | default ($item | upsert threshold false)
      return {out: $emit, next: {reached: true}}
    }
    if ($item.pulse? | default false) {
      # Pulse: forward post-threshold (live keepalive); drop pre-threshold
      # so they don't pollute the replay buffer.
      if ("reached" in $state) { return {out: $item, next: $state} }
      return {next: $state}
    }
    if ("reached" in $state) {
      return {out: $item, next: $state}
    }
    {next: ($state | upsert last $item)}
  }
}

# Box C. Pure rendering. {state, mode, direction?, changed?} -> html record.
# Pulse markers pass through untouched so html-to-patches can emit them as
# datastar-patch-signals heartbeats.
def states-to-html [] {
  each {|s|
    if ($s.pulse? | default false) {
      $s
    } else if ('signals' in $s) {
      $s
    } else {
      $s.state | render-current $s.mode ($s.direction? | default "") ($s.changed? | default false) ($s.req_id? | default "")
    }
  }
}

# Box D. Wrap each render in a datastar patch event. Unique id per patch
# so morphdom recreates the .edge-flash element and its animation re-fires
# each step. Pulse markers become a no-op signal patch -- a heartbeat that
# script.js uses to refresh the connection-liveness timer.
def html-to-patches [] {
  each {|item|
    if ($item.pulse? | default false) {
      {} | to datastar-patch-signals
    } else if ('signals' in $item) {
      $item.signals | to datastar-patch-signals
    } else {
      $item | to datastar-patch-elements --use-view-transition --id (random uuid)
    }
  }
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      # The player's "current game" is always the most-recent frame on
      # their games topic. /move derives it server-side so the client
      # doesn't have to track gameId at all -- it just sends intents.
      let signals = $in | from datastar-signals $req
      let player_id = $signals | get playerId? | default ""
      let intent = $signals | get intent? | default ""
      let req_id = $signals | get reqId? | default ""
      let games_topic = $"player.($player_id).games"
      if $intent == "reset" {
        # Append to the index. The SSE pipeline picks this up, resets its
        # state machine, and streams a fresh board over the existing
        # connection -- no page reload needed. Carry req_id so the client's
        # RTT match finds the resulting fresh-board mutation.
        null | .append $games_topic --meta {req_id: $req_id}
      } else {
        let game_id = (.last $games_topic | get id)
        let topic = $"game.($game_id).move"
        if $intent == "undo" {
          null | .append $topic --meta {kind: "undo" req_id: $req_id}
        } else if $intent == "" {
          # RTT-measurement ping: client sends an empty intent so the SSE
          # echoes a no-op state. Carries no game-state effect, so don't
          # persist -- ephemeral keeps the move log clean for replay.
          null | .append $topic --ttl ephemeral --meta {intent: $intent req_id: $req_id}
        } else {
          # Covers h/j/k/l and slam-X.
          null | .append $topic --meta {intent: $intent req_id: $req_id}
        }
      }
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse"} {|req ctx|
      let signals = "" | from datastar-signals $req
      let player_id = $signals | get playerId? | default ""
      let games_topic = $"player.($player_id).games"
      # Orphan cookie: the player's index is empty (store wiped). Clear
      # the cookie and reload so GET / mints fresh identity.
      let current_game_frame = (try { .last $games_topic } catch { null })
      if $current_game_frame == null {
        "/" | to datastar-redirect | cookie delete "player" | to sse
      } else {
        let game_id = $current_game_frame.id
        # resume-game encapsulates the snapshot lookup + fallback. .state
        # seeds the accumulator's stack; .follow_from_id is the --after
        # cursor (exclusive) so we don't re-apply the move that produced
        # the snapshot.
        let resumed = (resume-game $game_id)
        # --pulse 450 injects xs.pulse frames into the stream every 450ms
        # which the pipeline turns into datastar-patch-signals heartbeats
        # for the client's liveness timer.
        .cat --follow --pulse 450 --after $resumed.follow_from_id
        | filter-for-player $games_topic
        | impulses-to-states {
          stack: [$resumed.state]
          mode: "game"
          game_id: $game_id
          games_topic: $games_topic
          started: (date now)
        }
        # pace-slam-steps AFTER threshold-gate-states: pre-threshold the gate
        # buffers (only keeps the last item), so paced replay steps get
        # dropped before they reach pace -- no sleeps fire during replay.
        # Post-threshold the gate forwards each item and pace inserts its
        # 200ms gap between consecutive paced ones for the live animation.
        | threshold-gate-states
        | pace-slam-steps
        # Snapshot tap writes game.<id>.snapshot (ttl last:1) on every
        # state-changing emit. Only the owning connection should call this;
        # viewers (when added) must skip it. The default_move_id is used
        # for threshold-only snapshots that have no source move.
        | snapshot-tap $game_id $player_id $current_game_frame.id
        | states-to-html
        | html-to-patches
        | to sse
      }
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

    (route {method: GET path: "/games"} {|req ctx|
      # Read-only history view. Latest first. Replays each game's move log
      # to derive its final state, then renders a thumbnail + summary.
      let cookies = $req | cookie parse
      let prior = $cookies | get player? | default ""
      let player_id = if ($prior | is-empty) { random uuid } else { $prior }
      let games_topic = $"player.($player_id).games"
      let games = (try { .cat -T $games_topic | reverse } catch { [] })
      (HTML
      (HEAD
        (META {charset: "utf-8"})
        (META {name: "viewport" content: "width=device-width, initial-scale=1"})
        (LINK {rel: "icon" href: "data:,"})
        (TITLE "past games -- 2048.nu")
        (LINK {rel: "stylesheet" href: ($req | href $"/styles.css?v=($REV)")}))
      (BODY {class: "games-view"}
        (H1 "past games")
        (P (A {href: ($req | href "/")} "\u{2190} play current"))
        (if ($games | is-empty) {
          (P {class: "hint"} "no games yet.")
        } else {
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
        })))
    })

    (route {method: POST path: "/view"} {|req ctx|
      # View changes go on the current game's move topic, but with
      # ttl=ephemeral so they are NOT persisted -- only currently-connected
      # subscribers receive them. Reconnecting always starts in game mode.
      let signals = $in | from datastar-signals $req
      let player_id = $signals | get playerId? | default ""
      let game_id = (.last $"player.($player_id).games" | get id)
      let topic = $"game.($game_id).move"
      let mode = $signals | get mode? | default "game"
      null | .append $topic --ttl ephemeral --meta {kind: "view" mode: $mode}
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/"} {|req ctx|
      # Player identity lives in a cookie; a refresh or new tab continues
      # the player's most-recent game. The player's games topic is an
      # index of all their games; the SSE pipeline picks the latest. If
      # the index is empty (new player or wiped store) we seed it here.
      let cookies = $req | cookie parse
      let prior = $cookies | get player? | default ""
      let player_id = if ($prior | is-empty) { random uuid } else { $prior }
      let games_topic = $"player.($player_id).games"
      if (try { (.last $games_topic) == null } catch { true }) {
        null | .append $games_topic
      }
      # Render an EMPTY board as the placeholder: same dimensions (grid cells
      # fill it) so no layout jump, but no tiles in the DOM yet. When the SSE
      # init patch arrives with the real tiles, they're unpaired (:only-child)
      # which fires the spawn pop-in -- otherwise paired tiles would just
      # cross-fade with no animation.
      let placeholder = {tiles: [] next_id: 1 score: 0 game_over: false} | render-game
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
        # playerId comes from the cookie. The current gameId lives entirely
        # server-side (latest frame in player.<id>.games); /move and /sse
        # derive it from the index, so the client only carries the player.
        # data-conn is managed by script.js based on SSE heartbeats; CSS
        # reacts via body[data-conn="down"] selectors.
        "data-player-id": $player_id
        "data-move-url": ($req | href "/move")
        "data-view-url": ($req | href "/view")
        "data-signals": $"{playerId: '($player_id)'}"
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
        ", or swipe. Undo: "
        (BUTTON {type: "button" "data-intent": "undo"} "u")
        ", reset: "
        (BUTTON {type: "button" "data-intent": "reset"} "r")
        ". "
        (A {href: ($req | href "/games")} "past games"))
      # data-init and data-indicator live on .column (which is never patched)
      # so the SSE fetch + connection signal survive the wholesale replacement
      # of #game's contents on every server patch.
      (DIV {
        class: "column"
        # data-sse tags this element so script.js can filter datastar-fetch
        # events to just our SSE (ignoring any unrelated @get/@post).
        "data-sse": ""
        "data-init": ("@get('" + ($req | href "/sse") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
      }
        # #game is the single view; SSE patches morph it between the game
        # board render and the settings panel render based on per-tab mode
        # in the event log.
        $placeholder
        (FOOTER
          (SPAN {class: "status"}
            (SPAN {id: "conn" title: "SSE connection"})
            (SPAN {id: "replay" title: "last SSE replay time"
              "data-text": "$replayMs ? `replayed in ${$replayMs}ms` : ''"} "")
            (SPAN {id: "rtt" title: "last move round-trip time"} ""))
          (SPAN {class: "credit"}
            (A {href: "https://http-nu.cross.stream"}
              "served by http-nu "
              (IMG {src: ($req | href "/ellie.png") alt: "ellie" class: "mascot"}))))))
      # Persist the player id for a year, refreshing on every visit.
      # --no-secure so the cookie works over plain HTTP for local dev.
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })
  ]
}
