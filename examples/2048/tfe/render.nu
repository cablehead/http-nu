# 2048 rendering. Pure: takes a game state record, returns an html DSL
# record (or a `{__html}` envelope). The SSE pipeline and the /play and
# /games routes are the consumers.

use http-nu/html *

# Used by `layout` below to resolve templates relative to this module.
const TEMPLATES_DIR = path self | path dirname | path join "templates"

# Cirulli's original palette: gold ramp from cream (2) to amber (1024+),
# tile text dark for low values, cream for high. Placeholder while we
# rethink tiles from first principles around the http-nu blue/cream
# aesthetic.
export def palette-for [v: int]: nothing -> record {
  let bg = match $v {
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
  let fg = if $v <= 4 { "#776e65" } else { "#f9f6f2" }
  {bg: $bg, fg: $fg}
}

# The board: a self-contained component. Single class `.board` on the
# root; layout, palette, and cell styling live in `.board > *` /
# `.board > div:not(:empty)` selectors in styles.css. Used at full
# size on /play and inside game-card thumbnails on /games.
#
# Compiled once into a minijinja template since render-board is hot --
# every snapshot push re-renders one or more boards, and the runtime
# HTML DSL is significantly slower per tile.
#
# Template layers, in DOM order (later layers paint on top):
#   1. 16 hardcoded empty cells (background grid; never change).
#   2. Ghosts -- one per tile consumed by a merge on this snapshot's
#      move. Same view-transition-name as the consumed tile, placed at
#      the merge cell with opacity 0. The browser pairs the visible
#      old tile with this invisible new ghost and slides it into the
#      merge cell while fading, instead of popping out of existence.
#   3. Tiles (live game-state tiles).
# Module-level `let` isn't allowed, and `.mj compile` isn't const-eval'able,
# so templates are built once per `use` via export-env and stashed in $env.
# Nushell allows only one export-env block per module, so both templates
# get compiled here.
export-env {
  $env.BOARD_TPL = .mj compile --inline (
    DIV {class: "board"}
      (0..3 | each {|r| 0..3 | each {|c|
        DIV {style: $"grid-column: ($c + 1); grid-row: ($r + 1);"} ""
      } } | flatten)
      (_for {g: "ghosts"} (DIV {
        style: "grid-column: {{ g.col }}; grid-row: {{ g.row }}; background-color: {{ g.bg }}; view-transition-name: {{ g.vt }}; view-transition-class: ghost; opacity: 0; pointer-events: none;"
      } ""))
      (_for {t: "tiles"} (DIV {
        class: "{{ t.cls }}"
        style: "grid-column: {{ t.col }}; grid-row: {{ t.row }}; background-color: {{ t.bg }}; color: {{ t.fg }}; font-size: {{ t.fs }}cqw; view-transition-name: {{ t.vt }}; view-transition-class: {{ t.vt_class }};"
      } (_var "t.value")))
  )
  # Page-shell template (layout.html). Used once per request to wrap the
  # body content in <html><head>...</head><body>. See `layout` below.
  $env.LAYOUT_TPL = .mj compile ($TEMPLATES_DIR | path join "layout.html")
  # Fx tuner overlay (vt-tuner.html). Static markup + script; rendered
  # with no template vars. See `render-tuner` below.
  $env.TUNER_TPL = .mj compile ($TEMPLATES_DIR | path join "vt-tuner.html")
}

export def render-board [scope?: string]: record -> record {
  let state = $in
  let s = $scope | default ""
  # view-transition-name is page-global; on /games multiple boards share
  # a page, so the optional scope (game id) keeps names unique.
  let vt_name = {|id| if ($s | is-empty) { $"tile-($id)" } else { $"tile-($s)-($id)" }}
  let max_v = if ($state.tiles | is-empty) { 0 } else { $state.tiles | get value | math max }
  let tiles = $state.tiles | each {|t|
    let p = palette-for $t.value
    {
      col: ($t.c + 1)
      row: ($t.r + 1)
      bg: $p.bg
      fg: $p.fg
      fs: (if $t.value >= 1024 { 5 } else if $t.value >= 128 { 6 } else { 7 })
      vt: (do $vt_name $t.id)
      vt_class: (
        if ($t | get -o spawned | default false) { "spawned" }
        else if ($t | get -o merged | default false) { "merged" }
        else { "none" }
      )
      # `is-max` marks the highest-value tile(s). On splash thumbnails the
      # board mutes everything except this class, so the headline tile
      # pops out without a separate max-tile badge.
      cls: (if $t.value == $max_v { "is-max" } else { "" })
      value: $t.value
    }
  }
  # `ghosts` may be absent on snapshots written before the merge-ghosts
  # feature -- default to empty so old games still render.
  let ghosts = ($state | get ghosts? | default []) | each {|g|
    {col: ($g.c + 1) row: ($g.r + 1) bg: ((palette-for $g.value).bg) vt: (do $vt_name $g.id)}
  }
  {__html: ({tiles: $tiles, ghosts: $ghosts} | .mj render $env.BOARD_TPL)}
}

# Small targeted SSE fragments. Spans with stable ids morph in place on
# each state change.
export def render-score [score: int]: nothing -> record {
  (SPAN {id: "score"} ($score | into string))
}

# TEMPORARY: floating fx tuner overlay. Six dials controlling the VT-only
# pipeline (slide -> merge pop -> spawn). Remove this once the dial values
# are settled and the knobs become plain CSS constants.
export def render-tuner []: nothing -> record {
  {__html: ({} | .mj render $env.TUNER_TPL)}
}

# Bracketed key-cap button: `[ h ]`, `[ esc ]`, etc. The reusable shape
# behind every kbd hint in the UI (help panel rows, breadcrumb esc hint,
# new-game shortcut on /). Renders as either a <button> (with optional
# data-intent picked up by the click delegate in script.js) or an <a>
# (when --href is supplied -- navigation shortcuts).
#   --bracketless: drop the [ ] frame entirely (for `fx`, which isn't a
#   keypress so the brackets would be misleading).
export def kbd-btn [
  label: string
  --intent: string = ""
  --href: string = ""
  --class: string = ""
  --bracketless
]: nothing -> record {
  let inner = if $bracketless {
    [(SPAN {class: "key"} $label)]
  } else {
    [(SPAN {class: "bracket"} "[") (SPAN {class: "key"} $label) (SPAN {class: "bracket"} "]")]
  }
  let cls = ["kbd-btn" $class] | where {|c| ($c | str trim | is-not-empty)} | str join " "
  if ($href | is-not-empty) {
    (A {class: $cls href: $href} ...$inner)
  } else if ($intent | is-not-empty) {
    (BUTTON {type: "button" class: $cls "data-intent": $intent} ...$inner)
  } else {
    (BUTTON {type: "button" class: $cls} ...$inner)
  }
}

export def render-state-badge [won: bool, game_over: bool]: nothing -> record {
  if $game_over {
    (SPAN {id: "state-badge" class: "badge over"} "game over")
  } else if $won {
    (SPAN {id: "state-badge" class: "badge win"} "you win!")
  } else {
    (SPAN {id: "state-badge"} "")
  }
}

export def render-game [direction?: string, changed?: bool, req_id?: string]: record -> record {
  let state = $in
  # The edge-glow color rides the highest-value tile, pushed as an inline
  # CSS variable so it cascades to #board-wrap and the ::after pseudo.
  let glow = (palette-for (if ($state.tiles | is-empty) { 2 } else { $state.tiles | get value | math max })).bg
  let dir = $direction | default ""
  let did_change = $changed | default false
  let rid = $req_id | default ""
  (DIV {
    id: "game"
    style: $"--glow: ($glow); view-transition-name: view-game;"
    "data-rev": (if ($rid | is-empty) { random uuid } else { $rid })
    "data-from": $dir
    "data-changed": (if $did_change { "1" } else { "" })
  }
    # data-pending is set client-side on keydown and cleared when the SSE
    # patch lands; preserve it across morphs so the edge glow stays lit
    # for the duration of the round trip.
    (DIV {id: "board-wrap" "data-preserve-attr": "class data-pending"} ($state | render-board)))
}

# Render a card from already-known state. Callers pass state straight
# out of a snapshot frame's meta, avoiding a redundant resume-game lookup.
# Render a SCRU128 id's embedded timestamp as a short, human-readable
# string. Under a minute reads as "in play" (the game is still warm);
# beyond that it's "Xm ago" / "Xh ago" / "Xd ago" / "Xw ago".
# `.id unpack` is the http-nu builtin (no subprocess).
def last-active-from-id [id: string]: nothing -> string {
  let ts = .id unpack $id | get timestamp
  let diff = ((date now) - $ts | into int) / 1_000_000_000 | math floor
  if $diff < 60 { "in play"
  } else if $diff < 3600 { $"(($diff / 60) | into int)m ago"
  } else if $diff < 86400 { $"(($diff / 3600) | into int)h ago"
  } else if $diff < 604800 { $"(($diff / 86400) | into int)d ago"
  } else { $"(($diff / 604800) | into int)w ago" }
}

# Each card answers "should I jump back into this one?". The thumbnail
# is the densest signal; the board itself mutes every tile except the
# highest value, so the headline ("how far this game got") emerges from
# the board without needing a separate max-tile badge. Two overlays sit
# on top: the last-active relative time ("in play" when fresh) and, when
# applicable, a fun rotated status badge (won / over).
export def render-card-from-state [
  req: record
  game_id: string
  state: record
  moves: int
  last_move_id?: string
]: nothing -> record {
  let max_tile = if ($state.tiles | is-empty) { 2 } else {
    $state.tiles | get value | math max
  }
  let lmid = $last_move_id | default $game_id
  let active = last-active-from-id $lmid
  # Raw timestamp lets the client ticker recompute "Xs ago" every few
  # seconds without a server round-trip; see updateActiveLabels() in
  # script.js.
  let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
  let status = if $max_tile >= 2048 { "won" } else if $state.game_over { "over" } else { "" }
  (A {id: $"card-($game_id)" class: "game-card" href: ($req | href $"/play/($game_id)")}
    (DIV {class: "board-wrap"} ($state | render-board $game_id))
    (SPAN {class: "overlay active" "data-played-ms": ($played_ms | into string)} $active)
    (if ($status | is-not-empty) { (SPAN {class: $"overlay badge ($status)"} $status) } else { "" }))
}

# Render the whole .games-list from an in-memory {game_id: snapshot_meta}
# record. Sort by game_id (scru128, time-ordered) desc so newest is first.
export def render-games-list-from-data [req: record, data: record]: nothing -> record {
  let entries = $data | transpose game_id meta | sort-by game_id --reverse
  (DIV {class: "games-list"} ($entries | each {|e|
    render-card-from-state $req $e.game_id $e.meta.state ($e.meta | get moves? | default 0) ($e.meta | get last_move_id? | default $e.game_id)
  }))
}

# Page shell. Takes a list of body children (html DSL records) and wraps
# them in the shared <html><head>...</head><body> from layout.html.
#
#   [(DIV ...) (FOOTER ...)] | layout $req $REV --title "..." --body-class "play"
#
# DATASTAR_JS_PATH is a const exported by http-nu/datastar; pass it in so
# this module doesn't depend on http-nu/datastar being in scope.
export def layout [
  req: record
  rev: string
  datastar_src: string
  --title: string = "nu2048"
  --og-image: string = ""
  --og-description: string = ""
  --body-class: string = ""
  --body-attrs: record = {}
]: list -> string {
  let children = $in
  let body_html = $children | each {|c| $c.__html } | str join
  {
    title: $title
    og_image: $og_image
    og_description: $og_description
    styles_href: ($req | href $"/styles.css?v=($rev)")
    datastar_src: $datastar_src
    script_src: ($req | href $"/script.js?v=($rev)")
    ellie_href: ($req | href "/ellie.png")
    splash_href: ($req | href "/")
    body_class: $body_class
    body_attrs: ($body_attrs | transpose key value)
    body_html: $body_html
  } | .mj render $env.LAYOUT_TPL
}
