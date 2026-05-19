# 2048 rendering. Pure: takes a game state record, returns an html DSL
# record (or a `{__html}` envelope). The SSE pipeline and the /play and
# /games routes are the consumers.

use http-nu/html *
use http-nu/http *

# Used by `layout` below to resolve templates relative to this module.
const TEMPLATES_DIR = path self | path dirname | path join "templates"

# Pluck the wire-format view of a game state for the <game-board> WC.
# Keeps each tile's animation hints (spawned / merged) and the ghosts
# list (consumed-this-move + their merge destinations) so the WC can
# animate from snapshot annotations alone -- no client-side diff
# against a previous state, no prevState bookkeeping.
export def state-for-wc []: record -> record {
  let s = $in
  {
    tiles: ($s.tiles | each {|t| {
      id: $t.id
      r: $t.r
      c: $t.c
      value: $t.value
      spawned: ($t | get spawned? | default false)
      merged: ($t | get merged? | default false)
    } })
    ghosts: ($s | get ghosts? | default [] | each {|g|
      {id: $g.id r: $g.r c: $g.c value: $g.value}
    })
    gameOver: ($s | get game_over? | default false)
  }
}

# Page-shell template (layout.html). Used once per request to wrap the
# body content in <html><head>...</head><body>. See `layout` below.
# Module-level `let` isn't allowed and `.mj compile` isn't const-eval'able,
# so templates are built once per `use` via export-env and stashed in $env.
export-env {
  $env.LAYOUT_TPL = .mj compile ($TEMPLATES_DIR | path join "layout.html")
}

# Small targeted SSE fragments. Spans with stable ids morph in place on
# each state change.
export def render-score [score: int]: nothing -> record {
  # Bound to the $score signal: Datastar's text plugin overwrites
  # textContent on mount and on every signal patch, so post-init
  # score updates flow as signals patches rather than element morphs.
  (SPAN {id: "score" "data-text": "$score"} ($score | into string))
}

# Breadcrumb header: a one-row nav element shared by / and /play. Left
# side holds the path (page title + optional crumbs); right side holds
# action shortcuts (kbd-btns). Callers pass each side as a list of HTML
# DSL records.
export def breadcrumb [
  --left: list = []
  --right: list = []
]: nothing -> record {
  (NAV {class: "breadcrumb"}
    (DIV {class: "left"} ...$left)
    (DIV {class: "right"} ...$right))
}

# Bracketed key-cap button. The phrase is the button; the keyboard
# shortcut sits inside the phrase as `[k]`. Examples:
#   kbd-btn "h"                              -> [h]              (key is whole label)
#   kbd-btn "esc" --suffix " home"           -> [esc] home       (key + descriptive tail)
#   kbd-btn "n" --suffix "ew game"           -> [n]ew game       (key is first letter)
#   kbd-btn "p" --prefix "(()) " --suffix "lay"
#                                            -> (()) [p]lay      (key inside phrase)
#   kbd-btn "play now" --variant primary     -> [ play now ]     (CTA, no specific key)
#
# Renders <a class="kbd-btn"> when --href is set (so right-click-open-tab
# works) and <button class="kbd-btn"> otherwise. Behavior carriers:
#   --intent "h"|"undo"|...  fires move(intent) via script.js delegate
#   --href   "/new"|"/"|...  the <a>'s real href
#   neither                  caller wires a custom handler via --class
#
# --variant "primary" picks the orange CTA palette (splash play-now).
# Default variant is subdued; both flip to their accent on :hover and
# on [aria-pressed="true"] (so toggle state reuses the hover treatment).
export def kbd-btn [
  label: string                  # the key (or whole label if no prefix/suffix)
  --intent: string = ""
  --href: string = ""
  --class: string = ""
  --prefix: string = ""           # text before the [
  --suffix: string = ""           # text after the ]
  --variant: string = "default"   # "default" | "primary"
  --aria-label: string = ""
  --style: string = ""            # inline per-instance tweak (margin, etc.)
]: nothing -> record {
  let bracketed = [
    (SPAN {class: "bracket"} "[")
    (SPAN {class: "key"} $label)
    (SPAN {class: "bracket"} "]")
  ]
  mut inner = []
  if ($prefix | is-not-empty) { $inner = ($inner | append (SPAN {class: "phrase"} $prefix)) }
  $inner = ($inner | append $bracketed)
  if ($suffix | is-not-empty) { $inner = ($inner | append (SPAN {class: "phrase"} $suffix)) }
  let variant_class = if ($variant == "primary") { "primary" } else { "" }
  let cls = ["kbd-btn" $variant_class $class] | where {|c| ($c | str trim | is-not-empty)} | str join " "
  let elem = if ($href | is-not-empty) { "A" } else { "BUTTON" }
  mut attrs = {class: $cls}
  if $elem == "BUTTON" { $attrs = ($attrs | upsert "type" "button") }
  if ($intent | is-not-empty) { $attrs = ($attrs | upsert "data-intent" $intent) }
  if ($href | is-not-empty)   { $attrs = ($attrs | upsert "href" $href) }
  if ($aria_label | is-not-empty) { $attrs = ($attrs | upsert "aria-label" $aria_label) }
  if ($style | is-not-empty)      { $attrs = ($attrs | upsert "style" $style) }
  if $elem == "A" { (A $attrs ...$inner) } else { (BUTTON $attrs ...$inner) }
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
  --href: string  # destination URL (mount-resolved by caller); defaults to /play
]: nothing -> record {
  let target = if ($href | is-empty) { ($req | href $"/play/($game_id)") } else { $href }
  # Each card binds to two nested signals keyed by game id:
  #   $games[<id>] = {tiles: [...], gameOver: <bool>}  -> WC board
  #   $meta[<id>]  = {playedMs}                          -> overlay time
  # The WC's shadow DOM owns the won/over badge (derived from
  # boardState), so there's no external badge element per card.
  let g = "['" + $game_id + "']"
  let board_expr = "JSON.stringify($games" + $g + ")"
  let played_expr = "$meta" + $g + ".playedMs"
  (A {id: $"card-($game_id)" class: "game-card" href: $target}
    (DIV {class: "board-wrap"}
      (render-tag "game-board" {"data-attr:state": $board_expr dim: ""}))
    (SPAN {
      class: "overlay active"
      "data-attr:data-played-ms": $played_expr
    } ""))
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
  --sse = false
  --head-extra: list = []   # extra HTML records spliced into <head> (after the
                            # core <link>/<script> tags). Used by sub-sites
                            # like /design to add per-section stylesheets or
                            # ES modules without forking the page shell.
]: list -> string {
  let children = $in
  let body_html = $children | each {|c| $c.__html } | str join
  let head_extra_html = $head_extra | each {|c|
    let d = $c | describe -d | get type
    if $d == "record" and ('__html' in $c) { $c.__html } else { "" }
  } | str join
  # Short user slug for the header chip; empty string = no chip shown
  # (template guards on `{% if player_id %}`). Reads the `session`
  # cookie and looks up the bound user_id -- never the cookie token.
  let token = ($req | cookie parse | get session? | default "")
  let pid = if ($token | is-empty) { "" } else {
    let f = try { .last $"session.($token)" } catch { null }
    if $f == null { "" } else { $f.meta | get user_id? | default "" }
  }
  let pid_short = if ($pid | is-empty) { "" } else { $pid | str substring 0..7 }
  # script.js can't see the request, so resolved nav URLs ride along as
  # body data-* attrs. Keyboard handlers there read these instead of
  # hardcoded "/" / "/new", so Esc and n work under any mount prefix.
  let nav_attrs = {
    "data-home-href": ($req | href "/")
    "data-new-href":  ($req | href "/new")
  }
  {
    title: $title
    og_image: $og_image
    og_description: $og_description
    styles_href: ($req | href $"/styles.css?v=($rev)")
    datastar_src: $datastar_src
    script_src: ($req | href $"/script.js?v=($rev)")
    game_board_src: ($req | href $"/game-board.js?v=($rev)")
    ellie_href: ($req | href "/ellie.png")
    splash_href: ($req | href "/")
    my_games_href: ($req | href "/my/games")
    design_href: ($req | href "/design/")
    player_id: $pid_short
    sse: $sse
    body_class: $body_class
    body_attrs: ($nav_attrs | merge $body_attrs | transpose key value)
    head_extra: $head_extra_html
    body_html: $body_html
  } | .mj render $env.LAYOUT_TPL
}
