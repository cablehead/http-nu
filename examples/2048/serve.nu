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
# render-board is a hot path: every snapshot push re-renders one or more
# boards. Building the markup via the runtime HTML DSL costs ~40ms per
# board (most of it nushell record construction per tile/cell). With
# multiple cards re-rendering on /games it adds up to perceptible lag.
#
# Compile a minijinja template once at module load and render through it
# instead -- ~1ms per board, ~40x faster, identical output. The 16 empty
# background cells are hardcoded into the template (they never change);
# tiles are a {% for %} loop driven by a pre-shaped list computed in
# render-board.
## The board template has three layers:
##   1. 16 hardcoded empty cells (background grid).
##   2. ghosts -- one per tile consumed by a merge on the move that
##      produced this snapshot. Same `view-transition-name` as the
##      consumed tile, placed at the merge cell with opacity 0.
##      The browser pairs the old visible tile with this new invisible
##      ghost and slides it into the merge position before fading,
##      instead of just popping out of existence.
##   3. tiles (the live game-state tiles).
let BOARD_TPL = .mj compile --inline (
  DIV {class: "board"}
    (0..3 | each {|r| 0..3 | each {|c|
      DIV {style: $"grid-column: ($c + 1); grid-row: ($r + 1);"} ""
    } } | flatten)
    (_for {g: "ghosts"} (DIV {
      style: "grid-column: {{ g.col }}; grid-row: {{ g.row }}; background-color: {{ g.bg }}; view-transition-name: {{ g.vt }}; view-transition-class: ghost; opacity: 0; pointer-events: none;"
    } ""))
    (_for {t: "tiles"} (DIV {
      style: "grid-column: {{ t.col }}; grid-row: {{ t.row }}; background-color: {{ t.bg }}; color: {{ t.fg }}; font-size: {{ t.fs }}cqw; view-transition-name: {{ t.vt }};"
    } (_var "t.value")))
)

def render-board [scope?: string]: record -> record {
  let state = $in
  let s = $scope | default ""
  # view-transition-name is page-global; on /games multiple boards share
  # a page, so the optional scope (game id) keeps names unique.
  let vt_name = {|id| if ($s | is-empty) { $"tile-($id)" } else { $"tile-($s)-($id)" }}
  # Per-tile arithmetic in nushell (cheap); loop body emitted by the
  # compiled template in Rust (fast).
  let tiles = $state.tiles | each {|t|
    {
      col: ($t.c + 1)
      row: ($t.r + 1)
      bg: (color-for $t.value)
      fg: (if $t.value <= 4 { "#776e65" } else { "#f9f6f2" })
      fs: (if $t.value >= 1024 { 5 } else if $t.value >= 128 { 6 } else { 7 })
      vt: (do $vt_name $t.id)
      value: $t.value
    }
  }
  # `ghosts` may be absent on snapshots written before the merge-ghosts
  # feature -- default to empty so old games still render.
  let ghosts = ($state | get ghosts? | default []) | each {|g|
    {col: ($g.c + 1) row: ($g.r + 1) bg: (color-for $g.value) vt: (do $vt_name $g.id)}
  }
  {__html: ({tiles: $tiles, ghosts: $ghosts} | .mj render $BOARD_TPL)}
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
    "data-from": $dir
    "data-changed": (if $did_change { "1" } else { "" })
  }
    (DIV {id: "board-wrap" "data-preserve-attr": "class"} ...$wrap_children))
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
      let board = ($state | render-game ($s.direction? | default "") ($s.changed? | default false) ($s.req_id? | default ""))
      [
        {vt: true, el: $board}
        {vt: false, el: (render-score $state.score)}
        {vt: false, el: (render-state-badge $won $state.game_over)}
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
        # every datastar POST so /move doesn't need to look them up
        # server-side. data-conn is managed by script.js based on SSE
        # heartbeats; CSS reacts via body[data-conn="down"] selectors.
        "data-player-id": $player_id
        "data-game-id": $game_id
        "data-move-url": ($req | href "/move")
        "data-signals": $"{playerId: '($player_id)', gameId: '($game_id)'}"
      }
        (DIV {class: "page"}
          (HEADER {class: "play-header"}
            (DIV {class: "left"}
              (A {href: $home_href} "← back")
              (SPAN {class: "hint"} "keys: hjkl / arrows")
              (BUTTON {type: "button" "data-intent": "undo" class: "linklike"} "undo"))
            (DIV {class: "right"}
              (SPAN {class: "score-label"} "score ")
              (render-score 0)
              (render-state-badge false false)))
          # data-init lives on .column (which is never patched) so the SSE
          # fetch + connection signal survive the morph of #game's contents.
          (DIV {
            class: "column"
            "data-sse": ""
            "data-init": ("@get('" + ($req | href $"/sse/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
          }
            $placeholder)
          (FOOTER {class: "play-footer"}
            (DIV {class: "left"}
              (SPAN {id: "conn" class: "stat"} "")
              (SPAN {id: "rtt" class: "stat"} ""))
            (DIV {class: "right"}
              (SPAN {class: "credit"}
                (A {href: "https://http-nu.cross.stream"}
                  "served by http-nu "
                  (IMG {src: ($req | href "/ellie.png") alt: "ellie" class: "mascot"}))))))
        # Temporary tuning panel for the view-transition bezier dials.
        # Lives as a floating overlay so it doesn''t disturb the layout.
        # Each slider writes its CSS custom property on :root and the
        # SVG preview re-plots the bezier in motion-y space (y=0 at the
        # bottom is "not yet moved", y=1 mid-height is "at target",
        # anything above the target line is overshoot). Remove this
        # block once we''ve settled on values.
        {__html: '<aside class="vt-tuner">
  <button class="vt-tuner-tab" aria-label="toggle tuner"><span class="vt-tuner-tab-arrow" aria-hidden="true">&#9656;</span><span class="vt-tuner-tab-label">fx</span></button>
  <div class="vt-tuner-body">
    <div class="vt-tuner-actions">
      <button class="vt-tuner-btn vt-tuner-copy">copy json</button>
      <button class="vt-tuner-btn vt-tuner-reset">reset</button>
      <span class="vt-tuner-flash" data-flash></span>
    </div>
    <details class="vt-tuner-fold" open>
      <summary>anticipation <span class="vt-tuner-sub">wind-up + spring-back</span></summary>
      <svg class="vt-tuner-plot" viewBox="0 0 100 100" preserveAspectRatio="none">
        <line class="ax-zero"   x1="0" y1="50" x2="100" y2="50"/>
        <line class="ax-target" x1="0" y1="85" x2="100" y2="85"/>
        <path class="curve" data-curve-ant fill="none" stroke-width="2"/>
      </svg>
      <div class="vt-tuner-legend">
        <div>y = board tilt (0 = level, peak = lean, &lt;0 = recoil past level)</div>
        <div>each dial is <em>base</em> + <em>k</em>&middot;rtt; set k=0 to ignore latency.</div>
      </div>

      <div class="vt-tuner-section">duration <span class="vt-tuner-sub">total lean time, ms</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="80" max="1200" step="10" value="200" data-var="--ant-duration-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; stretches the lean</span></span><span data-readout></span><input type="range" min="-1" max="5" step="0.05" value="2" data-var="--ant-duration-k"></label>
      </div>

      <div class="vt-tuner-section">magnitude <span class="vt-tuner-sub">how far the board leans, in tile widths (1 = a whole tile)</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0" max="1.5" step="0.01" value="0.02" data-var="--ant-magnitude-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; bigger lean</span></span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--ant-magnitude-k"></label>
      </div>

      <div class="vt-tuner-section">y1 <span class="vt-tuner-sub">recoil height; &gt;1 = bounces past 0 the other way</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="1" max="3" step="0.05" value="2" data-var="--ant-y1-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; <em>shrink</em> recoil (neg.)</span></span><span data-readout></span><input type="range" min="-0.01" max="0.01" step="0.0005" value="0" data-var="--ant-y1-k"></label>
      </div>

      <div class="vt-tuner-section">attack <span class="vt-tuner-sub">(x1) how fast each half commits to its target; smaller = sooner peak</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0" max="1" step="0.01" value="0.05" data-var="--ant-x1-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; <em>earlier</em> peak (neg.)</span></span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--ant-x1-k"></label>
      </div>

      <div class="vt-tuner-section">decay <span class="vt-tuner-sub">(x2) how each half settles after the peak; larger = longer linger</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0" max="1" step="0.01" value="0.05" data-var="--ant-x2-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; longer linger</span></span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--ant-x2-k"></label>
      </div>

      <div class="vt-tuner-section">y2 <span class="vt-tuner-sub">final value; 1 = lands flat on target</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0.5" max="1.5" step="0.05" value="1" data-var="--ant-y2-base"></label>
        <label><span class="hdr">k</span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--ant-y2-k"></label>
      </div>
    </details>

    <details class="vt-tuner-fold" open>
      <summary>slide <span class="vt-tuner-sub">tile movement bezier</span></summary>
      <svg class="vt-tuner-plot" viewBox="0 0 100 100" preserveAspectRatio="none">
        <line class="ax-zero"   x1="0" y1="100" x2="100" y2="100"/>
        <line class="ax-target" x1="0" y1="50"  x2="100" y2="50"/>
        <path class="curve" data-curve fill="none" stroke-width="2"/>
      </svg>
      <div class="vt-tuner-legend">
        <div><strong>x</strong> = time (0&#8594;1)</div>
        <div><strong>y</strong> = motion (0 = start, 1 = at target, &gt;1 = overshoot)</div>
        <div>each dial is <em>base</em> + <em>k</em>&middot;rtt; set k=0 to ignore latency.</div>
        <div>rtt mean: <span data-rtt-readout>0</span>ms &rarr; effective <span data-bz>0.34, 1.56, 0.64, 1</span></div>
      </div>
      <div class="vt-tuner-section">duration <span class="vt-tuner-sub">total slide time, ms</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="80" max="600" step="10" value="100" data-var="--vt-duration-base" data-unit="ms"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; stretch</span></span><span data-readout></span><input type="range" min="-1" max="2" step="0.05" value="0.1" data-var="--vt-duration-k"></label>
      </div>

      <div class="vt-tuner-section">y1 <span class="vt-tuner-sub">overshoot height; 1 = none, &gt;1 = bounce past target</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="1" max="3" step="0.05" value="1.2" data-var="--vt-y1-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; <em>shrink</em> bounce (neg.)</span></span><span data-readout></span><input type="range" min="-0.01" max="0.01" step="0.0005" value="0.001" data-var="--vt-y1-k"></label>
      </div>

      <div class="vt-tuner-section">attack <span class="vt-tuner-sub">(x1) how fast it commits to the target; smaller = sooner peak</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0" max="1" step="0.01" value="0.34" data-var="--vt-x1-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; <em>earlier</em> peak (neg.)</span></span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--vt-x1-k"></label>
      </div>

      <div class="vt-tuner-section">decay <span class="vt-tuner-sub">(x2) how it settles after the peak; larger = softer landing</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0" max="1" step="0.01" value="0.64" data-var="--vt-x2-base"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; softer landing</span></span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--vt-x2-k"></label>
      </div>

      <div class="vt-tuner-section">y2 <span class="vt-tuner-sub">final value; 1 = lands flat on target</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="0.5" max="1.5" step="0.05" value="1" data-var="--vt-y2-base"></label>
        <label><span class="hdr">k</span><span data-readout></span><input type="range" min="-0.005" max="0.005" step="0.0001" value="0" data-var="--vt-y2-k"></label>
      </div>

      <div class="vt-tuner-section">ghost duration <span class="vt-tuner-sub">how long a merge-consumed tile lingers as it slides + fades into the merge cell (ms)</span></div>
      <div class="vt-tuner-pair">
        <label><span class="hdr">base</span><span data-readout></span><input type="range" min="120" max="2000" step="20" value="120" data-var="--ghost-duration-base" data-unit="ms"></label>
        <label><span class="hdr">k <span class="vt-tuner-sub">slow &rarr; longer ghost</span></span><span data-readout></span><input type="range" min="-1" max="3" step="0.05" value="0" data-var="--ghost-duration-k"></label>
      </div>
    </details>
  </div>
</aside>
<script>
(() => {
  const root = document.documentElement;
  const tuner = document.querySelector(".vt-tuner");
  const inputs = tuner.querySelectorAll("input[data-var]");
  const bz = tuner.querySelector("[data-bz]");
  const rttReadout = tuner.querySelector("[data-rtt-readout]");
  const curve = tuner.querySelector("[data-curve]");
  const update = (input) => {
    const unit = input.dataset.unit || "";
    const v = input.value + unit;
    root.style.setProperty(input.dataset.var, v);
    input.previousElementSibling.textContent = v;
  };
  const curveAnt = tuner.querySelector("[data-curve-ant]");
  const sy = (my) => Math.max(0, Math.min(100, 100 - my * 50));
  // overshoot plot: motion-y in [0,2] -> SVG y in [100,0], target line at 50.
  // anticipation plot: tilt in [-1, 1] -> SVG y in [85, -15] (clamped to 0-100).
  // For anticipation, the keyframes are: 0% (tilt=0), 15% (tilt=peak), 100% (tilt=0)
  // with the bezier easing applied to the 15%->100% segment.
  const clamp = (lo, v, hi) => Math.max(lo, Math.min(hi, v));
  const dials = {
    duration: { lo: 80,   hi: 600,  unit: "ms" },
    y1:       { lo: 1.0,  hi: 3.0 },
    x1:       { lo: 0.05, hi: 0.95 },
    x2:       { lo: 0.05, hi: 0.95 },
    y2:       { lo: 0.5,  hi: 1.5 },
  };
  // Anticipation plot: build a polyline of the synthetic 0->15%->100%
  // keyframe by sampling the bezier on the spring-back segment.
  const sampleBezier = (x1, y1, x2, y2, n = 24) => {
    const points = [];
    for (let i = 0; i <= n; i++) {
      const t = i / n;
      const mt = 1 - t;
      // cubic-bezier y at parametric t (not the same as time progress;
      // but for a curve preview this approximates the shape well).
      const y = 3*mt*mt*t*y1 + 3*mt*t*t*y2 + t*t*t*1;
      const x = 3*mt*mt*t*x1 + 3*mt*t*t*x2 + t*t*t*1;
      points.push([x, y]);
    }
    return points;
  };
  const refresh = () => {
    const cs = getComputedStyle(root);
    const rtt = parseFloat(cs.getPropertyValue("--rtt-mean")) || 0;
    rttReadout.textContent = Math.round(rtt);
    // -- overshoot curve --
    const eff = {};
    for (const [name, { lo, hi }] of Object.entries(dials)) {
      const base = parseFloat(cs.getPropertyValue(`--vt-${name}-base`));
      const k = parseFloat(cs.getPropertyValue(`--vt-${name}-k`));
      eff[name] = clamp(lo, base + k * rtt, hi);
    }
    bz.textContent = [eff.x1.toFixed(3), eff.y1.toFixed(3), eff.x2.toFixed(3), eff.y2.toFixed(3)].join(", ");
    curve.setAttribute("d", `M 0,${sy(0)} C ${eff.x1*100},${sy(eff.y1)} ${eff.x2*100},${sy(eff.y2)} 100,${sy(1)}`);
    // -- anticipation curve: 0 -> peak (15% time) -> 0 (with spring bezier) --
    // Plot tilt over time. y-axis in SVG: 50 = level (0 tilt), 85 = peak
    // lean, anything above 50 = recoil past 0. Mapping tilt t in [-1, 1.5]
    // to SVG y: SVG_y = 50 + t * 35, clamped to [0, 100].
    const antDials = {
      "ant-y1": { lo: 1.0,  hi: 3.0 },
      "ant-x1": { lo: 0.05, hi: 0.95 },
      "ant-x2": { lo: 0.05, hi: 0.95 },
      "ant-y2": { lo: 0.5,  hi: 1.5 },
    };
    const ant = {};
    for (const [name, { lo, hi }] of Object.entries(antDials)) {
      const base = parseFloat(cs.getPropertyValue(`--${name}-base`));
      const k = parseFloat(cs.getPropertyValue(`--${name}-k`));
      ant[name] = clamp(lo, base + k * rtt, hi);
    }
    const ax1 = ant["ant-x1"];
    const ay1 = ant["ant-y1"];
    const ax2 = ant["ant-x2"];
    const ay2 = ant["ant-y2"];
    const ty = (tilt) => Math.max(0, Math.min(100, 50 + tilt * 35));
    // Both segments share the SAME bezier (it lives on the
    // animation-timing-function and applies to every keyframe pair). So
    // overshoot-above-peak in the wind-up and overshoot-past-zero in
    // the recoil are both consequences of dialling x1/y1/x2/y2.
    const peakX = 15;
    const samples = sampleBezier(ax1, ay1, ax2, ay2, 24);
    let d = "";
    // Wind-up 0 -> peak over 0..15% of x: tilt = bezier_y * peak (1).
    samples.forEach(([bx, by], i) => {
      const cmd = i === 0 ? "M" : "L";
      d += ` ${cmd} ${(bx * peakX).toFixed(2)},${ty(by).toFixed(2)}`;
    });
    // Recoil peak -> 0 over 15..100%: tilt = peak - bezier_y * peak.
    for (const [bx, by] of samples) {
      d += ` L ${(peakX + bx * (100 - peakX)).toFixed(2)},${ty(1 - by).toFixed(2)}`;
    }
    curveAnt.setAttribute("d", d);
  };
  // Persistence: serialise every input value (keyed by --css-var name)
  // to localStorage on every change. Load on boot before the initial
  // update() pass so the CSS vars take the stored values.
  const KEY_SETTINGS = "fx-tuner-settings";
  const defaults = {};
  inputs.forEach(i => { defaults[i.dataset.var] = i.value; });
  const loadSettings = () => {
    try {
      const stored = JSON.parse(localStorage.getItem(KEY_SETTINGS) || "{}");
      inputs.forEach(i => {
        if (i.dataset.var in stored) i.value = stored[i.dataset.var];
      });
    } catch {}
  };
  const saveSettings = () => {
    const out = {};
    inputs.forEach(i => { out[i.dataset.var] = i.value; });
    try { localStorage.setItem(KEY_SETTINGS, JSON.stringify(out)); } catch {}
  };
  loadSettings();
  inputs.forEach(i => { update(i); i.addEventListener("input", () => { update(i); saveSettings(); refresh(); }); });
  refresh();
  new MutationObserver(refresh).observe(root, { attributes: true, attributeFilter: ["style"] });

  // Copy + reset buttons.
  const flash = tuner.querySelector("[data-flash]");
  const flashMsg = (msg) => {
    flash.textContent = msg;
    clearTimeout(flash.__t);
    flash.__t = setTimeout(() => { flash.textContent = ""; }, 1500);
  };
  tuner.querySelector(".vt-tuner-copy").addEventListener("click", async () => {
    const out = {};
    inputs.forEach(i => {
      const v = i.value;
      out[i.dataset.var] = /^-?\d+(\.\d+)?$/.test(v) ? parseFloat(v) : v;
    });
    const json = JSON.stringify(out, null, 2);
    try { await navigator.clipboard.writeText(json); flashMsg("copied"); }
    catch { flashMsg("copy failed"); }
  });
  tuner.querySelector(".vt-tuner-reset").addEventListener("click", () => {
    inputs.forEach(i => { i.value = defaults[i.dataset.var]; update(i); });
    saveSettings();
    refresh();
    flashMsg("reset");
  });

  // Slide-out behaviour: the tab on the right edge of the viewport stays
  // visible; clicking it toggles the body in/out via a class on the aside.
  // localStorage keeps the panel state across reloads.
  const KEY = "vt-tuner-collapsed";
  if (localStorage.getItem(KEY) === "1") tuner.classList.add("is-collapsed");
  tuner.querySelector(".vt-tuner-tab").addEventListener("click", () => {
    const collapsed = tuner.classList.toggle("is-collapsed");
    localStorage.setItem(KEY, collapsed ? "1" : "0");
  });
})();
</script>'})
      # Persist the player id for a year, refreshing on every visit.
      # --no-secure so the cookie works over plain HTTP for local dev.
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })
  ]
}
