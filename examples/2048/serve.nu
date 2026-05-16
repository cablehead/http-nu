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

# Register the xs snapshot-actor (singleton): it watches every
# `player.*.games` + `game.*.move` frame and writes the canonical
# `game.<id>.snapshot` (ttl: last:1). Requires `--store` + `--services`;
# guarded so that test.nu, which sources serve.nu without a store, stays
# happy. Re-registering on each startup replaces the running actor (per
# xs's `<name>.register` semantics), so this is restart-safe.
if ($HTTP_NU.store? | default null) != null and ($HTTP_NU.services? | default false) {
  open ($SCRIPT_DIR | path join "game.nu") | .append game.nu
  open ($SCRIPT_DIR | path join "snapshot-actor.nu") | .append snapshot-actor.register
}

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

def gear-button []: nothing -> record {
  (BUTTON {class: "track-btn track-gear" type: "button" aria-label: "settings" "data-view-to": "settings"}
    (ICONIFY "material-symbols:settings-outline-rounded" {width: "20" height: "20"}))
}

def close-button []: nothing -> record {
  (BUTTON {class: "track-btn track-gear" type: "button" aria-label: "close" "data-view-to": "game"}
    (ICONIFY "material-symbols:close-rounded" {width: "20" height: "20"}))
}

# Small targeted SSE fragments. The top tracker bar lives statically at
# the body level; these spans inside it have stable ids and are morphed
# in place on each state change. Keeping the bar out of #game means the
# board patch is small and the bar's layout doesn't need to round-trip
# nav links and the game-id chip on every move.
def render-score [score: int]: nothing -> record {
  (SPAN {id: "score" class: "track-value"} ($score | into string))
}

def render-state-badge [won: bool, game_over: bool]: nothing -> record {
  if $game_over {
    (SPAN {id: "state-badge" class: "track-field track-badge track-badge-over"} "GAME OVER")
  } else if $won {
    (SPAN {id: "state-badge" class: "track-field track-badge track-badge-win"} "YOU WIN!")
  } else {
    (SPAN {id: "state-badge"} "")
  }
}

def render-mode-toggle [mode: string]: nothing -> record {
  let btn = if $mode == "settings" { close-button } else { gear-button }
  (SPAN {id: "mode-toggle"} $btn)
}

def render-game [direction?: string, changed?: bool, req_id?: string]: record -> record {
  let state = $in
  # The edge-glow color rides the highest-value tile, pushed as an inline
  # CSS variable so it cascades to #board-wrap and the ::after pseudo.
  let glow = color-for (if ($state.tiles | is-empty) { 2 } else { $state.tiles | get value | math max })
  let dir = $direction | default ""
  let did_change = $changed | default false
  let rid = $req_id | default ""
  let wrap_children = if ($did_change and $dir in [h j k l]) {
    [
      ($state | render-board)
      (DIV {id: $"flash-(random uuid)" class: "edge-flash" "data-dir": $dir} "")
    ]
  } else {
    [($state | render-board)]
  }
  (DIV {
    id: "game"
    style: $"--glow: ($glow); view-transition-name: view-game;"
    "data-rev": (if ($rid | is-empty) { random uuid } else { $rid })
    "data-view": "game"
    "data-from": $dir
    "data-changed": (if $did_change { "1" } else { "" })
  }
    (DIV {id: "board-wrap"} ...$wrap_children))
}

def render-settings []: nothing -> record {
  (DIV {
    id: "game"
    style: "view-transition-name: view-settings;"
    "data-rev": (random uuid)
    "data-view": "settings"
  }
    (DIV {id: "settings-panel"}
      (H2 "settings")
      (P "more knobs soon.")))
}

# One card in the games list: thumbnail + score + max-tile + move count.
# Wraps in an anchor so clicking the card jumps into /play/<id>.
def render-game-card [req: record game_frame: record]: nothing -> record {
  let game_id = $game_frame.id
  let resumed = (resume-game $game_id)
  let state = $resumed.state
  let max_tile = if ($state.tiles | is-empty) { 0 } else {
    $state.tiles | get value | math max
  }
  let move_count = $resumed.moves
  let badge = if $max_tile >= 2048 {
    (DIV {class: "badge won"} "won")
  } else if $state.game_over {
    (DIV {class: "badge failed"} "failed")
  } else {
    (DIV {class: "badge paused"} "paused")
  }
  (A {class: "game-card" href: ($req | href $"/play/($game_id)")}
    (DIV {class: "thumb"} ($state | render-board))
    (DIV {class: "meta"}
      (DIV {class: "score"} $"Score: ($state.score)")
      (DIV {} $"Max tile: ($max_tile)")
      (DIV {} $"Moves: ($move_count)")
      $badge))
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
#   .cat --follow -> filter-for-player -> impulses-to-states
#   -> threshold-gate-states -> states-to-html
#   -> html-to-patches -> to sse
# Snapshot writes happen out-of-band in the xs snapshot-actor.
# Each stage has one job.

# Box A. impulses-to-states (and filter-for-player) live in mod.nu so they're
# reusable from `http-nu eval`. The remaining boxes here are SSE-specific:
# threshold gating, render, and patch wrapping.

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
# Each state expands into a list of 4 small renders (board + score + badge
# + mode-toggle). The board patch uses view-transition (so tiles slide);
# the bar fragments are tagged {vt: false} so they morph in place without
# kicking off their own view-transition (multiple VTs per state interrupt
# the tile slide animation).
def states-to-html [] {
  each {|s|
    if ($s.pulse? | default false) {
      [$s]
    } else if ('signals' in $s) {
      [$s]
    } else {
      let state = $s.state
      let won = $state.tiles | any {|t| $t.value >= 2048 }
      let board = ($state | render-current $s.mode ($s.direction? | default "") ($s.changed? | default false) ($s.req_id? | default ""))
      [
        {vt: true, el: $board}
        {vt: false, el: (render-score $state.score)}
        {vt: false, el: (render-state-badge $won $state.game_over)}
        {vt: false, el: (render-mode-toggle $s.mode)}
      ]
    }
  }
  | flatten
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
    } else if (('vt' in $item) and not $item.vt) {
      # Bar fragments: no view-transition. Plain morph in place so they
      # don't kick off a VT that would interrupt the board's tile slides.
      $item.el | to datastar-patch-elements --id (random uuid)
    } else {
      let el = if ('el' in $item) { $item.el } else { $item }
      $el | to datastar-patch-elements --use-view-transition --id (random uuid)
    }
  }
}

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (route {method: POST path: "/move"} {|req ctx|
      # The client carries the game id (URL-routed play view, so the
      # page knows which game it's on). Body shape: {playerId, gameId,
      # intent, reqId}.
      let signals = $in | from datastar-signals $req
      let game_id = $signals | get gameId? | default ""
      let intent = $signals | get intent? | default ""
      let req_id = $signals | get reqId? | default ""
      if ($game_id | is-empty) {
        null | metadata set { merge {'http.response': {status: 400}} }
      } else {
        let topic = $"game.($game_id).move"
        if $intent == "undo" {
          null | .append $topic --meta {kind: "undo" req_id: $req_id}
        } else if $intent == "" {
          # RTT-measurement ping: client sends an empty intent so the SSE
          # echoes a no-op state. Don't persist -- ephemeral keeps the
          # move log clean.
          null | .append $topic --ttl ephemeral --meta {intent: $intent req_id: $req_id}
        } else {
          # Covers h/j/k/l.
          null | .append $topic --meta {intent: $intent req_id: $req_id}
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      }
    })

    (route {method: GET path-matches: "/sse/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # The actor owns game state; the SSE handler is a thin reader of
      # this game's snapshot stream (plus ephemeral view-toggles).
      # --from $game_id includes the games_topic frame at that id and
      # everything after; the threshold-gate buffers it down to just the
      # latest snapshot for the initial render.
      .cat --follow --pulse 450 -T $"game.($game_id).*" --from $game_id
      | frames-to-states
      | threshold-gate-states
      | states-to-html
      | html-to-patches
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
      # View toggle on this game's move topic with ttl=ephemeral so only
      # currently-connected SSE subscribers see it. Game id comes from
      # the page's data-signals (URL-routed play view).
      let signals = $in | from datastar-signals $req
      let game_id = $signals | get gameId? | default ""
      let mode = $signals | get mode? | default "game"
      if ($game_id | is-not-empty) {
        null | .append $"game.($game_id).move" --ttl ephemeral --meta {kind: "view" mode: $mode}
      }
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/new"} {|req ctx|
      # Mint a games_topic frame for this player and 302 to /play/<id>.
      # Cookie minted on first visit.
      let cookies = $req | cookie parse
      let prior = $cookies | get player? | default ""
      let player_id = if ($prior | is-empty) { random uuid } else { $prior }
      let games_topic = $"player.($player_id).games"
      let new_frame = (null | .append $games_topic)
      let location = ($req | href $"/play/($new_frame.id)")
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $location}}} }
      | cookie set "player" $player_id --max-age 31536000 --no-secure
    })

    (route {method: GET path: "/"} {|req ctx|
      # Splash = past games + New game link. The play view lives at
      # /play/<game_id>. Mint a cookie on first visit so subsequent
      # actions (creating games, etc.) have a player identity.
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
        (TITLE "2048.nu")
        (LINK {rel: "stylesheet" href: ($req | href $"/styles.css?v=($REV)")}))
      (BODY {class: "games-view"}
        (H1 (A {href: "https://github.com/cablehead/http-nu/blob/main/examples/2048/serve.nu"} "2048.nu"))
        (P (A {class: "new-game-link" href: ($req | href "/new")} "+ New game"))
        (if ($games | is-empty) {
          (P {class: "hint"} "no games yet.")
        } else {
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
        }))
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })

    (route {method: GET path-matches: "/play/:game_id"} {|req ctx|
      let player_id = ($req | cookie parse | get player? | default (random uuid))
      let game_id = $ctx.game_id
      # Render an EMPTY board as the placeholder: same dimensions (grid cells
      # fill it) so no layout jump, but no tiles in the DOM yet. When the SSE
      # init patch arrives with the real tiles, they're unpaired (:only-child)
      # which fires the spawn pop-in -- otherwise paired tiles would just
      # cross-fade with no animation.
      let home_href = ($req | href "/")
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
        class: "tracker play"
        # playerId from cookie, gameId from URL path. Both ride along on
        # every datastar POST so /move and /view don't need to look them
        # up server-side. data-conn is managed by script.js based on SSE
        # heartbeats; CSS reacts via body[data-conn="down"] selectors.
        "data-player-id": $player_id
        "data-game-id": $game_id
        "data-move-url": ($req | href "/move")
        "data-view-url": ($req | href "/view")
        "data-signals": $"{playerId: '($player_id)', gameId: '($game_id)'}"
      }
      # Top tracker bar: stays static at body level. Live bits (score,
      # state badge, gear/close toggle) have their own ids and are
      # morphed in place by separate SSE patches emitted alongside the
      # board patch (see states-to-html).
      (DIV {class: "track-bar track-bar-top"}
        (SPAN {class: "track-title"} (A {href: $home_href} "2048.nu"))
        (SPAN {class: "track-field"}
          (SPAN {class: "track-label"} "Game ")
          (SPAN {class: "track-value"} ($game_id | str substring 0..7)))
        (SPAN {class: "track-field"}
          (SPAN {class: "track-label"} "Score ")
          (render-score 0))
        (SPAN {class: "track-field"}
          (SPAN {class: "track-label"} "Keys ")
          (SPAN {class: "track-value"} "hjkl/arrows"))
        (BUTTON {type: "button" "data-intent": "undo" class: "track-field track-action"}
          "["
          (SPAN {class: "track-key"} "u")
          "]ndo")
        (render-state-badge false false)
        (SPAN {class: "track-spacer"} "")
        (render-mode-toggle "game")
        (A {class: "track-nav" href: $home_href} "[Esc] All games"))
      # data-init and data-indicator live on .column (which is never patched)
      # so the SSE fetch + connection signal survive the wholesale replacement
      # of #game's contents on every server patch.
      (DIV {
        class: "column"
        # data-sse tags this element so script.js can filter datastar-fetch
        # events to just our SSE (ignoring any unrelated @get/@post).
        "data-sse": ""
        "data-init": ("@get('" + ($req | href $"/sse/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
      }
        # #game is the single view; SSE patches morph it between the game
        # board render and the settings panel render based on per-tab mode
        # in the event log.
        $placeholder)
      (DIV {class: "track-bar track-bar-bot"}
        (SPAN {class: "track-field"}
          (SPAN {class: "track-label"} "SSE ")
          (SPAN {id: "conn" class: "track-value" title: "SSE connection"} ""))
        (SPAN {class: "track-field"}
          (SPAN {class: "track-label"} "RTT ")
          (SPAN {id: "rtt" class: "track-value" title: "last move round-trip time"} ""))
        (SPAN {class: "track-spacer"} "")
        (SPAN {class: "track-credit"}
          (A {href: "https://http-nu.cross.stream"}
            "served by http-nu "
            (IMG {src: ($req | href "/ellie.png") alt: "ellie" class: "mascot"})))))
      # Persist the player id for a year, refreshing on every visit.
      # --no-secure so the cookie works over plain HTTP for local dev.
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })
  ]
}
