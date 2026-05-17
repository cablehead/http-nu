# 2048 rendering. Pure: takes a game state record, returns an html DSL
# record (or a `{__html}` envelope). The SSE pipeline and the /play and
# /games routes are the consumers.

use http-nu/html *

# Used by `layout` below to resolve templates relative to this module.
const TEMPLATES_DIR = path self | path dirname | path join "templates"

# --- color palette: spectral cascade -----------------------------------
#
# Each tile is an octave of light, walking the EM spectrum from
# low-energy red (~1.8 eV) through the visible band into UV. The
# doubling of tile values is the doubling of photon energy. Hue, chroma
# and lightness are tabulated at spectral landmarks rather than fitted
# to a curve. Chromas are pushed a hair past the original swatch
# explorer to make each tile commit to its color.
#
# Foreground is picked at table-build time via in-hue + WCAG gate:
# propose a fg in the same hue family (same H, low C, opposite L);
# verify contrast ratio >= 4.5:1 against the bg; push L further until
# it passes. Result: numbers stay tinted with their tile's color, never
# bleached white or black, and contrast is provable.

const SPECTRAL_STOPS = [
  #  L     C    H      value: description
  [0.50 0.21  30.0]   # 2:    deep red          (~656nm, H-alpha)
  [0.62 0.22  40.0]   # 4:    orange-red
  [0.70 0.21  55.0]   # 8:    orange
  [0.78 0.20  85.0]   # 16:   yellow            (~580nm, sodium D)
  [0.85 0.19 110.0]   # 32:   yellow-green
  [0.82 0.23 140.0]   # 64:   green             (peak eye sensitivity)
  [0.75 0.18 200.0]   # 128:  cyan              (~486nm, H-beta)
  [0.62 0.23 245.0]   # 256:  blue
  [0.50 0.24 280.0]   # 512:  indigo            (~434nm, H-gamma)
  [0.42 0.23 305.0]   # 1024: violet            (~410nm, H-delta)
  [0.32 0.21 320.0]   # 2048: deep violet       (visible edge)
  [0.22 0.14 320.0]   # 4096: near-UV           (fading from sight)
  [0.10 0.06 320.0]   # 8192: deep UV           (off-spectrum void)
]
const SPECTRAL_VALUES = [2 4 8 16 32 64 128 256 512 1024 2048 4096 8192]

# String interpolation `$"oklch(($a) ($b) ...)"` confuses nu's parser
# (adjacent `($x) ($y)` reads as function application). Join args first.
def oklch [l: float, c: float, h: float]: nothing -> string {
  'oklch(' + ([$l $c $h] | str join ' ') + ')'
}

# OKLCH -> linear sRGB -> WCAG relative luminance Y. Bjorn Ottosson's
# OKLab matrices; out-of-gamut channels clip to [0,1] before the
# luminance sum -- coarse but enough for a pass/fail contrast check.
def oklch-to-luminance [l: float, c: float, h: float]: nothing -> float {
  let hr = $h * 3.141592653589793 / 180
  let a = $c * ($hr | math cos)
  let b = $c * ($hr | math sin)
  let lp = $l + 0.3963377774 * $a + 0.2158037573 * $b
  let mp = $l - 0.1055613458 * $a - 0.0638541728 * $b
  let sp = $l - 0.0894841775 * $a - 1.2914855480 * $b
  let ll = $lp * $lp * $lp
  let mm = $mp * $mp * $mp
  let ss = $sp * $sp * $sp
  let r =  4.0767416621 * $ll - 3.3077115913 * $mm + 0.2309699292 * $ss
  let g = -1.2684380046 * $ll + 2.6097574011 * $mm - 0.3413193965 * $ss
  let b2 = -0.0041960863 * $ll - 0.7034186147 * $mm + 1.7076147010 * $ss
  let clamp = {|v| if $v < 0 { 0.0 } else if $v > 1 { 1.0 } else { $v } }
  0.2126 * (do $clamp $r) + 0.7152 * (do $clamp $g) + 0.0722 * (do $clamp $b2)
}

def wcag-ratio [y1: float, y2: float]: nothing -> float {
  let lo = if $y1 < $y2 { $y1 } else { $y2 }
  let hi = if $y1 < $y2 { $y2 } else { $y1 }
  ($hi + 0.05) / ($lo + 0.05)
}

# In-hue fg proposal, WCAG-gated. Same H as bg, low C (0.06), L pushed
# toward the opposite extreme until contrast >= 4.5:1.
def fg-pick [bg_l: float, bg_c: float, bg_h: float]: nothing -> string {
  let bg_y = oklch-to-luminance $bg_l $bg_c $bg_h
  let candidates = if $bg_l > 0.5 { [0.20 0.15 0.10 0.05] } else { [0.95 0.97 0.99 1.0] }
  mut picked = $candidates | last
  for lp in $candidates {
    let y = oklch-to-luminance $lp 0.06 $bg_h
    if (wcag-ratio $bg_y $y) >= 4.5 { $picked = $lp; break }
  }
  oklch $picked 0.06 $bg_h
}

# {value: {bg, fg}} lookup, built once at module load.
def build-palette-lut []: nothing -> record {
  $SPECTRAL_VALUES | enumerate | reduce -f {} {|p acc|
    let s = $SPECTRAL_STOPS | get $p.index
    let l = $s | get 0
    let c = $s | get 1
    let h = $s | get 2
    $acc | upsert ($p.item | into string) {bg: (oklch $l $c $h), fg: (fg-pick $l $c $h)}
  }
}

# Look up the palette pair for a tile value. Past the largest tabulated
# value, clamp to the last entry (the off-spectrum void).
export def palette-for [v: int]: nothing -> record {
  let max_v = $SPECTRAL_VALUES | last
  let key = if $v > $max_v { $max_v } else { $v } | into string
  $env.PALETTE_LUT | get $key
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
        style: "grid-column: {{ t.col }}; grid-row: {{ t.row }}; background-color: {{ t.bg }}; color: {{ t.fg }}; font-size: {{ t.fs }}cqw; view-transition-name: {{ t.vt }}; view-transition-class: {{ t.vt_class }};"
      } (_var "t.value")))
  )
  # Page-shell template (layout.html). Used once per request to wrap the
  # body content in <html><head>...</head><body>. See `layout` below.
  $env.LAYOUT_TPL = .mj compile ($TEMPLATES_DIR | path join "layout.html")
  # Fx tuner overlay (vt-tuner.html). Static markup + script; rendered
  # with no template vars. See `render-tuner` below.
  $env.TUNER_TPL = .mj compile ($TEMPLATES_DIR | path join "vt-tuner.html")
  # Spectral cascade palette LUT. Computing fg-pick per render would
  # iterate WCAG contrast for every tile; precompute once here.
  $env.PALETTE_LUT = build-palette-lut
}

export def render-board [scope?: string]: record -> record {
  let state = $in
  let s = $scope | default ""
  # view-transition-name is page-global; on /games multiple boards share
  # a page, so the optional scope (game id) keeps names unique.
  let vt_name = {|id| if ($s | is-empty) { $"tile-($id)" } else { $"tile-($s)-($id)" }}
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
export def render-card-from-state [req: record game_id: string state: record moves: int]: nothing -> record {
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

# Render the whole .games-list from an in-memory {game_id: snapshot_meta}
# record. Sort by game_id (scru128, time-ordered) desc so newest is first.
export def render-games-list-from-data [req: record, data: record]: nothing -> record {
  let entries = $data | transpose game_id meta | sort-by game_id --reverse
  (DIV {class: "games-list"} ($entries | each {|e|
    render-card-from-state $req $e.game_id $e.meta.state ($e.meta | get moves? | default 0)
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
  --title: string = "2048.nu"
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
    body_class: $body_class
    body_attrs: ($body_attrs | transpose key value)
    body_html: $body_html
  } | .mj render $env.LAYOUT_TPL
}
