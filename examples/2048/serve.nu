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

# The board: a self-contained component. Single class `.board` on the
# root; layout, palette, and cell styling all live in `.board > *` /
# `.board > div:not(:empty)` selectors in styles.css. Used at full size
# on /play and CSS-scaled inside game-card thumbnails on /games -- same
# render, different wrap.
#
# Per-cell inline styles are limited to what's actually data-driven:
# grid placement (r,c), tile bg/color/font-size (value), and the
# view-transition-name (tile id). Empty cells render as `<div></div>` and
# get their look from the structural `.board > div:empty` selector.
def render-tile [scope?: string]: record -> record {
  let t = $in
  let s = $scope | default ""
  # view-transition-name is page-global; on /games multiple boards live
  # on the same page, so the optional scope (game id) keeps names unique.
  let vt_name = if ($s | is-empty) { $"tile-($t.id)" } else { $"tile-($s)-($t.id)" }
  (DIV {style: {
    grid-column: ($t.c + 1 | into string)
    grid-row: ($t.r + 1 | into string)
    background-color: (color-for $t.value)
    color: (if $t.value <= 4 { "#776e65" } else { "#f9f6f2" })
    font-size: (if $t.value >= 1024 { "24px" } else if $t.value >= 128 { "28px" } else { "32px" })
    view-transition-name: $vt_name
  }} ($t.value | into string))
}

def render-empty-cell [r: int c: int]: nothing -> record {
  (DIV {style: {
    grid-column: ($c + 1 | into string)
    grid-row: ($r + 1 | into string)
  }} "")
}

def render-board [scope?: string]: record -> record {
  let state = $in
  let bg = 0..3 | each {|r| 0..3 | each {|c| render-empty-cell $r $c } } | flatten
  let tiles = $state.tiles | each {|t| $t | render-tile $scope }
  (DIV {class: "board"} $bg $tiles)
}

def gear-button []: nothing -> record {
  (BUTTON {class: "icon-btn" type: "button" aria-label: "settings" "data-view-to": "settings"}
    (ICONIFY "material-symbols:settings-outline-rounded" {width: "18" height: "18"}))
}

def close-button []: nothing -> record {
  (BUTTON {class: "icon-btn" type: "button" aria-label: "close" "data-view-to": "game"}
    (ICONIFY "material-symbols:close-rounded" {width: "18" height: "18"}))
}

# Small targeted SSE fragments. The top tracker bar lives statically at
# the body level; these spans inside it have stable ids and are morphed
# in place on each state change. Keeping the bar out of #game means the
# board patch is small and the bar's layout doesn't need to round-trip
# nav links and the game-id chip on every move.
def render-score [score: int]: nothing -> record {
  (SPAN {id: "score"} ($score | into string))
}

def render-state-badge [won: bool, game_over: bool]: nothing -> record {
  if $game_over {
    (SPAN {id: "state-badge" class: "badge over"} "game over")
  } else if $won {
    (SPAN {id: "state-badge" class: "badge win"} "you win!")
  } else {
    (SPAN {id: "state-badge"} "")
  }
}

def render-mode-toggle [mode: string]: nothing -> record {
  let btn = if $mode == "settings" { close-button } else { gear-button }
  (SPAN {id: "mode-toggle"} $btn)
}

# Shared site footer used by both /play and /games so SSE liveness shows
# everywhere. Optional `actions` go on the left (e.g. undo button + key
# hint on /play); the status bits + credit are always on the right.
def render-footer [req: record, actions: list = []]: nothing -> record {
  (FOOTER {class: "site-footer"}
    ...$actions
    (SPAN {class: "spacer"} "")
    (SPAN {id: "conn" class: "stat"} "")
    (SPAN {id: "rtt" class: "stat"} "")
    (SPAN {class: "credit"}
      (A {href: "https://http-nu.cross.stream"}
        "served by http-nu "
        (IMG {src: ($req | href "/ellie.png") alt: "ellie" class: "mascot"}))))
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

# Render a card from already-known state. The SSE handler calls this with
# state straight out of the snapshot frame's meta, avoiding a redundant
# resume-game lookup per live update.
def render-card-from-state [req: record game_id: string state: record moves: int]: nothing -> record {
  let max_tile = if ($state.tiles | is-empty) { 0 } else {
    $state.tiles | get value | math max
  }
  let status = if $max_tile >= 2048 { "won" } else if $state.game_over { "over" } else { "" }
  let caption_bits = [
    $"score ($state.score)"
    $"moves ($moves)"
    (if ($status | is-not-empty) { $status } else { null })
  ] | compact
  (A {id: $"card-($game_id)" class: "game-card" href: ($req | href $"/play/($game_id)")}
    (DIV {class: "thumb"} ($state | render-board $game_id))
    (DIV {class: "caption"} ($caption_bits | str join " · ")))
}

# Render a card from a games_topic frame (the initial page render). Resumes
# the game to get state, then defers to render-card-from-state.
def render-game-card [req: record game_frame: record]: nothing -> record {
  let resumed = (resume-game $game_frame.id)
  render-card-from-state $req $game_frame.id $resumed.state $resumed.moves
}

# Render the whole .games-list from an in-memory {game_id: snapshot_meta}
# record. Sort by game_id (scru128, time-ordered) desc so newest is first.
def render-games-list-from-data [req: record, data: record]: nothing -> record {
  let entries = $data | transpose game_id meta | sort-by game_id --reverse
  (DIV {class: "games-list"} ($entries | each {|e|
    render-card-from-state $req $e.game_id $e.meta.state ($e.meta | get moves? | default 0)
  }))
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

# Top-of-pipeline keepalive. xs.pulse frames are turned into ready-to-send
# datastar-patch-signals event records immediately, so downstream rendering
# stages never have to know about pulses -- they just pass through anything
# that already has an `event` field. Reused by every SSE handler in this
# file.
def pulse-keepalive [] {
  each {|f|
    if ($f.topic? | default "") == "xs.pulse" {
      ({} | to datastar-patch-signals)
    } else { $f }
  }
}

# Box B. Buffers states pre-threshold (only the last is retained); on
# threshold marker emits the last buffered state; then forwards everything.
# Pre-converted SSE event records pass through untouched (they're already
# ready for `to sse`).
def threshold-gate-states [] {
  generate {|item state = {}|
    if ('event' in $item) {
      return {out: $item, next: $state}
    }
    if ($item.threshold? | default false) {
      let emit = $state.last? | default ($item | upsert threshold false)
      return {out: $emit, next: {reached: true}}
    }
    if ("reached" in $state) {
      return {out: $item, next: $state}
    }
    {next: ($state | upsert last $item)}
  }
}

# Box C. Pure rendering. Each state expands into a list of 4 small renders
# (board + score + badge + mode-toggle). The board patch uses view-
# transition (so tiles slide); the bar fragments are tagged {vt: false}
# so they morph in place without kicking off their own VT (multiple VTs
# per state interrupt the tile slide animation). Already-event items
# pass through unchanged.
def states-to-html [] {
  each {|s|
    if ('event' in $s) {
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
# each step. Already-event items pass through unchanged.
def html-to-patches [] {
  each {|item|
    if ('event' in $item) {
      $item
    } else if ('signals' in $item) {
      $item.signals | to datastar-patch-signals
    } else if (('vt' in $item) and not $item.vt) {
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

    (route {method: GET path: "/sse/games"} {|req ctx|
      # Live updates for the all-games splash.
      #
      # Strategy: keep an in-memory {game_id: snapshot_meta} record as
      # the generate accumulator. On a snapshot frame the meta itself
      # IS the new state -- just upsert. On a new games_topic frame
      # (player.<id>.games), pull the root snapshot the actor just
      # wrote and add it. Either path re-renders the whole .games-list
      # from $data and emits a single morph patch. morphdom diffs the
      # new HTML against the DOM so unchanged cards don't mutate.
      let cookies = $req | cookie parse
      let player_id = $cookies | get player? | default ""
      let games_topic = $"player.($player_id).games"
      if ($player_id | is-empty) {
        null | metadata set { merge {'http.response': {status: 400}} }
      } else {
        let head = (try { .cat | last | get id? } catch { null })
        # Seed $data with the player's existing games + their latest
        # snapshots so the first push is consistent with the initial
        # page render.
        let initial_data = (try { .cat -T $games_topic } catch { [] })
          | reduce -f {} {|f acc|
              let snap = try { .last $"game.($f.id).snapshot" } catch { null }
              if $snap == null { $acc } else { $acc | upsert $f.id $snap.meta }
            }
        .cat --follow --pulse 450
        | pulse-keepalive
        | generate {|item data|
            if ('event' in $item) { return {out: $item, next: $data} }
            if ($head != null and $item.id <= $head) { return {next: $data} }
            let new_data = if $item.topic == $games_topic {
              let snap = try { .last $"game.($item.id).snapshot" } catch { null }
              if $snap == null { $data } else { $data | upsert $item.id $snap.meta }
            } else if (($item.topic | str ends-with ".snapshot") and (($item.meta? | get player_id? | default "") == $player_id)) {
              let game_id = $item.topic | str replace "game." "" | str replace ".snapshot" ""
              $data | upsert $game_id $item.meta
            } else { $data }
            if $new_data == $data { return {next: $data} }
            let patch = (render-games-list-from-data $req $new_data
                          | to datastar-patch-elements --selector ".games-list" --use-view-transition --id (random uuid))
            {out: $patch, next: $new_data}
          } $initial_data
        | to sse
      }
    })

    (route {method: GET path-matches: "/sse/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # The actor owns game state; the SSE handler is a thin reader of
      # this game's snapshot stream (plus ephemeral view-toggles).
      # --from $game_id includes the games_topic frame at that id and
      # everything after; the threshold-gate buffers it down to just the
      # latest snapshot for the initial render. `pulse-keepalive` runs
      # first so the rendering stages can stay pulse-agnostic.
      .cat --follow --pulse 450 -T $"game.($game_id).*" --from $game_id
      | pulse-keepalive
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
        (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
        (SCRIPT {src: ($req | href $"/script.js?v=($REV)") defer: true})
        (LINK {rel: "stylesheet" href: ($req | href $"/styles.css?v=($REV)")}))
      (BODY {
        class: "games-view"
        # Live updates: subscribe to snapshot events for this player's games
        # and morph each #card-<game_id> in place as moves land. The
        # data-sse marker matches what script.js's connection-state tracker
        # listens to, so the #conn indicator works on this page too.
        "data-sse": ""
        "data-init": ("@get('" + ($req | href "/sse/games") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
      }
        (H1 "past games")
        (P (A {href: ($req | href "/new")} "+ new game"))
        # Always render .games-list (even if empty) so the SSE handler
        # has a stable target to prepend new-game cards into. The hint
        # below is a sibling, hidden via CSS when .games-list has any
        # children.
        (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
        (P {class: "hint empty-state"} "no games yet.")
        (render-footer $req))
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
        class: "play"
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
        (HEADER {class: "play-header"}
          (A {href: $home_href} "← back")
          (SPAN "score "
            (render-score 0))
          (render-state-badge false false))
        # data-init lives on .column (which is never patched) so the SSE
        # fetch + connection signal survive the morph of #game's contents.
        (DIV {
          class: "column"
          "data-sse": ""
          "data-init": ("@get('" + ($req | href $"/sse/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
        }
          $placeholder)
        (render-footer $req [
          (BUTTON {type: "button" "data-intent": "undo" class: "linklike"} "undo")
          (SPAN {class: "hint"} "keys: hjkl / arrows")
          (render-mode-toggle "game")
        ]))
      # Persist the player id for a year, refreshing on every visit.
      # --no-secure so the cookie works over plain HTTP for local dev.
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })
  ]
}
