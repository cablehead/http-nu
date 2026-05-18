# /design: a 2-column component viewer for nu2048. Sidebar lists every
# component; the focused one renders on the right. Ctrl-N / Ctrl-P
# navigate the list. MPA -- each component is its own URL so deep-links
# work and back-forward feels native.
#
# Stories (sample invocations) live inline in this file: render-stories
# dispatches on the slug. Adding a new component = catalog entry + a
# match arm.

use http-nu/router *
use http-nu/html *
use http-nu/datastar *
use ../tfe/render.nu *
use ../tfe/game.nu *

const HERE = path self | path dirname

# Cache-buster shared with the main site's REV idiom. Fresh per server
# start, stable within a session. Lives at module level so it's set once
# when serve.nu sources this file.
let REV = random uuid | str substring 0..7

# A markdown sample that exercises every construct the /notes pages
# actually render. Edit this when adding a new construct to the notes.
const MD_SAMPLE = "## A section heading

The opening paragraph sets the topic. Sentences run a couple of clauses
long, mixing **bold** lead-ins with the occasional *italic* aside. Inline
[links](./backstory) thread through the text -- you can hover them, and
they pick up the accent color.

### A deeper subsection

Sometimes a single h2 needs more structure. h3s break out sub-topics
without claiming page-level weight.

A second paragraph follows. It runs long enough that line-height
matters: comfortable reading on a 65-character measure means the eye
returns cleanly to the next line.

**Bulleted lists** sit between paragraphs:

- A single-line bullet.
- A bullet with **bold** to mark its lead-in -- then a clause that runs
  a sentence long.
- A bullet that itself contains a [link](./the-rules) so the link
  treatment looks right inside a list too.

**Numbered lists** for explicit sequences:

1. First, the precondition.
2. Then, the move.
3. Finally, the consequence.

> Block quotes pull a sentence out of the flow. They give a passage
> different weight -- a callout for someone else's words or a key idea
> you want the eye to land on.

Inline `code` appears mid-sentence when a flag or symbol is the thing
under discussion (e.g. the `--raw` switch). Whole blocks of code earn
their own indent:

```
.cat --follow --pulse 450
| pulse-keepalive
| frames-to-states
| states-to-html
| to sse
```

A closing paragraph lands the section. The space below it before the
next h2 is the breathing room the page needs."

# Catalog: every component on the site, in nav order. Order = the
# sequence Ctrl-N / Ctrl-P walks through.
const CATALOG = [
  {slug: "kbd-btn"    title: "kbd-btn"    desc: "bracketed key-cap button: [ h ]. triggers a move or navigation."}
  {slug: "breadcrumb" title: "breadcrumb" desc: "header nav row. left = path crumbs, right = action shortcuts."}
  {slug: "board"      title: "board"      desc: "4x4 game grid. tiles, ghosts, dim mask, max-tile highlight."}
  {slug: "badge"      title: "badge"      desc: "rotated pill stamped on a board. won/over variants."}
  {slug: "board-wc"   title: "board (wc)" desc: "encapsulated <game-board> custom element. state in as a signal-driven attribute; component owns slide/merge/spawn animation."}
  {slug: "markdown"   title: "markdown"   desc: "the full set of markdown the /notes pages render: headings, prose, lists, links, code, quotes."}
]

# Curated board states for the WC playground. Each scenario has a
# `pre` (setup) and `post` (play) state. Tile ids match between pre/post
# inside a scenario so the WC diff produces a clean slide/merge/spawn;
# ids deliberately DON'T match across scenarios, so jumping between
# scenarios shows tiles fading out and respawning -- a useful stress
# test of the "consumed tile with no merge target" path.
const WC_STATES = {
  empty:      {tiles: []}
  two:        {tiles: [{id: 11, r: 1, c: 0, value: 2}, {id: 12, r: 2, c: 3, value: 2}]}
  slide:      {tiles: [{id: 11, r: 1, c: 0, value: 2}, {id: 12, r: 2, c: 0, value: 2}]}
  merge_pre:  {tiles: [{id: 21, r: 0, c: 0, value: 2}, {id: 22, r: 0, c: 1, value: 2}]}
  merge_post: {tiles: [{id: 21, r: 0, c: 0, value: 4}]}
  chain_pre:  {tiles: [{id: 31, r: 0, c: 0, value: 2}, {id: 32, r: 0, c: 1, value: 2}, {id: 33, r: 0, c: 2, value: 2}, {id: 34, r: 0, c: 3, value: 2}]}
  chain_post: {tiles: [{id: 31, r: 0, c: 0, value: 4}, {id: 33, r: 0, c: 1, value: 4}]}
  # A real-looking slide-left move: the row-0 pair merges (51 survives,
  # 52 consumed into it), 53/54/55 each slide one or two cells left
  # without merging, and tile 60 spawns into the now-empty (0, 3) cell.
  # Shows all three phases on one transition.
  move_pre:   {tiles: [
    {id: 51 r: 0 c: 1 value: 2}
    {id: 52 r: 0 c: 3 value: 2}
    {id: 53 r: 1 c: 2 value: 4}
    {id: 54 r: 2 c: 2 value: 8}
    {id: 55 r: 3 c: 1 value: 16}
  ]}
  move_post:  {tiles: [
    {id: 51 r: 0 c: 0 value: 4}
    {id: 53 r: 1 c: 0 value: 4}
    {id: 54 r: 2 c: 0 value: 8}
    {id: 55 r: 3 c: 0 value: 16}
    {id: 60 r: 0 c: 3 value: 2}
  ]}
  big:        {tiles: [
    {id: 41 r: 0 c: 0 value: 2}    {id: 42 r: 0 c: 1 value: 4}
    {id: 43 r: 0 c: 2 value: 8}    {id: 44 r: 0 c: 3 value: 16}
    {id: 45 r: 1 c: 0 value: 32}   {id: 46 r: 1 c: 1 value: 64}
    {id: 47 r: 1 c: 2 value: 128}  {id: 48 r: 1 c: 3 value: 256}
    {id: 49 r: 2 c: 0 value: 512}  {id: 50 r: 2 c: 1 value: 1024}
    {id: 51 r: 2 c: 2 value: 2048} {id: 52 r: 2 c: 3 value: 4}
  ]}
}

# Render one or more stories (sample invocations) for a slug. Returns a
# list of HTML DSL records to drop into the right column.
def render-stories [slug: string]: nothing -> list {
  match $slug {
    "kbd-btn" => [
      (story "move key (the whole label is the key)" [
        (kbd-btn "h" --intent "h")
        (kbd-btn "j" --intent "j")
        (kbd-btn "k" --intent "k")
        (kbd-btn "l" --intent "l")
      ])
      (story "key inside a phrase: [n]ew game, [esc] home" [
        (kbd-btn "esc" --suffix " home" --href "/")
        (kbd-btn "n" --suffix "ew game" --href "/new")
      ])
      (story "prefix + key + suffix: audio-toggle style" [
        (kbd-btn "p" --prefix "(()) " --suffix "lay" --aria-label "play audio")
      ])
      (story "primary variant: the splash CTA" [
        (kbd-btn "n" --prefix "Play " --suffix "ow" --variant primary --href "/new")
      ])
      (story "no-key label: meta controls like the fx toggle" [
        (kbd-btn "fx" --class "fx-toggle")
      ])
    ]
    "breadcrumb" => [
      (story "splash header: page title + action" [
        (breadcrumb
          --left [(A {href: "/"} "past games")]
          --right [(kbd-btn "n" --suffix "ew game" --href "/new")])
      ])
      (story "/play header: path with shortcut + game-id" [
        (breadcrumb
          --left [
            (kbd-btn "esc" --suffix " home" --href "/")
            (SPAN {class: "sep"} "·")
            (A {href: "/play/03g5xxxx" class: "game-id"} "03g5xxxx")
          ]
          --right [(kbd-btn "n" --suffix "ew game" --href "/new")])
      ])
    ]
    "board" => [
      (story "mid-game state with a 128 high tile" [
        (DIV {style: "width: 380px;"} ({
          tiles: [
            {id: 1 r: 0 c: 0 value: 2}
            {id: 2 r: 0 c: 1 value: 4}
            {id: 3 r: 1 c: 1 value: 8}
            {id: 4 r: 1 c: 2 value: 16}
            {id: 5 r: 2 c: 2 value: 32}
            {id: 6 r: 2 c: 3 value: 64}
            {id: 7 r: 3 c: 3 value: 128}
          ]
          ghosts: []
        } | render-board "design-mid"))
      ])
      (story "empty board (initial state placeholder)" [
        (DIV {style: "width: 380px;"} ({tiles: [] ghosts: []} | render-board "design-empty"))
      ])
    ]
    "badge" => [
      (story "won (rotated -6deg, green)" [
        (SPAN {class: "badge won"} "won")
      ])
      (story "over (rotated -3deg, red)" [
        (SPAN {class: "badge over"} "over")
      ])
    ]
    "board-wc" => [
      (SECTION {class: "story"
                "data-signals": $"{boardState: ($WC_STATES.empty | to json --raw)}"}
        (P {class: "label"} "click setup then play. the WC diffs by tile id and runs slide -> merge-pop -> spawn-in. switching scenarios resets ids so consumed tiles fade in place (no merge target).")
        (DIV {class: "wc-playground"}
          (DIV {class: "wc-board"}
            (render-tag "game-board" {"data-attr:state": "JSON.stringify($boardState)"}))
          (DIV {class: "wc-controls"}
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "spawn -- two tiles appear on an empty board")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.empty | to json --raw)"} "setup")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.two | to json --raw)"} "play"))
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "slide -- one tile travels across an empty row")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.two | to json --raw)"} "setup")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.slide | to json --raw)"} "play"))
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "merge -- two 2s slide together, double, pop")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.merge_pre | to json --raw)"} "setup")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.merge_post | to json --raw)"} "play"))
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "chain -- four 2s in a row become two 4s")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.chain_pre | to json --raw)"} "setup")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.chain_post | to json --raw)"} "play"))
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "move -- slide-left over a populated board: one merge, three pure slides, one spawn. all three phases on one transition.")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.move_pre | to json --raw)"} "setup")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.move_post | to json --raw)"} "play"))
            (DIV {class: "wc-scenario"}
              (P {class: "wc-scenario-label"} "big board -- palette + font-size ramp across all tile values")
              (BUTTON {class: "wc-btn" "data-on:click": $"$boardState = ($WC_STATES.empty | to json --raw)"} "clear")
              (BUTTON {class: "wc-btn primary" "data-on:click": $"$boardState = ($WC_STATES.big | to json --raw)"} "fill")))))
    ]
    "markdown" => [
      (story "rendered via .md, wrapped in .prose (same path as /notes pages)" [
        (DIV {class: "prose"} {__html: ($MD_SAMPLE | .md | get __html)})
      ])
    ]
    _ => []
  }
}

# One labeled story block: caption above, rendered children below.
def story [label: string children: list]: nothing -> record {
  (SECTION {class: "story"}
    (P {class: "label"} $label)
    (DIV {class: "render"} ...$children))
}

# Page chrome. Two columns: sidebar with the catalog, main with the
# focused stories. Sidebar entries get .current on the active slug.
# Goes through the shared `layout` so Datastar (and the site header /
# footer) are present on every design page.
def design-page [req: record current: string]: nothing -> string {
  let stories = render-stories $current
  let entry = $CATALOG | where slug == $current | first
  # JS array literal with single-quoted strings -- avoids double-quote
  # collision with the outer $"..." interpolation.
  let slugs_js = "[" + ($CATALOG | get slug | each {|s| $"'($s)'"} | str join ", ") + "]"
  # The shared layout's asset URLs (styles, datastar, script, ellie,
  # splash, my-games) live at the parent app's root, NOT under /design.
  # Strip just the /design segment so $req|href in layout resolves to
  # the parent app's root. Body links keep $req with the full prefix.
  let root_req = $req | upsert mount_prefix ($req.mount_prefix | str replace -r '/design$' '')
  let head_extra = [
    # design.css is served by an explicit route below; browser-relative
    # `./design.css` resolves to /design/design.css from /design/<slug>.
    # game-board.js is now loaded by the shared layout, so no per-slug
    # extra is needed here.
    (LINK {rel: "stylesheet" href: "./design.css"})
  ]
  # Slug breadcrumb sits BELOW the layout's site-header. Browser-relative
  # hrefs: at /design/<slug>, `../` is /design/ and the bare slug self-
  # links inside /design/.
  ([
    (DIV {class: "page"}
      (breadcrumb
        --left [
          (A {href: "../"} "design")
          (SPAN {class: "sep"} "·")
          (A {href: $current} $entry.title)
        ]
        --right [])
      (MAIN {class: "design-main"}
        (NAV {class: "design-nav"}
          ($CATALOG | each {|c|
            (A {href: $c.slug class: (if $c.slug == $current { "current" } else { "" })}
              (SPAN {class: "title"} $c.title)
              (SPAN {class: "desc"} $c.desc))
          }))
        (SECTION {class: "design-preview"}
          ...$stories)))
    (SCRIPT {} {__html: ($"const slugs = ($slugs_js);\n" + (r#'const current = document.body.dataset.slug;
document.addEventListener('keydown', e => {
  if (!e.ctrlKey || e.metaKey || e.altKey) return;
  const i = slugs.indexOf(current);
  if (i < 0) return;
  if (e.key === 'n') { location.href = slugs[(i + 1) % slugs.length]; e.preventDefault(); }
  if (e.key === 'p') { location.href = slugs[(i - 1 + slugs.length) % slugs.length]; e.preventDefault(); }
});'#))})
  ] | layout $root_req $REV $DATASTAR_JS_PATH
        --title $"nu2048 / design / ($entry.title)"
        --body-class "design"
        --body-attrs {"data-slug": $current}
        --head-extra $head_extra)
}

{|req|
  dispatch $req [
    (route {method: GET path: "/"} {|req ctx|
      # / redirects to the first slug -- gives Ctrl-N/P a starting position.
      let first = $CATALOG | first | get slug
      let loc = $req | href $"/($first)"
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $loc}}} }
    })
    (route {method: GET path: "/design.css"} {|req ctx|
      .static $HERE "/design.css"
    })
    (route {method: GET path-matches: "/:slug"} {|req ctx|
      let known = $CATALOG | get slug
      if ($ctx.slug not-in $known) {
        "Not Found" | metadata set { merge {'http.response': {status: 404}} }
      } else {
        design-page $req $ctx.slug
      }
    })
  ]
}
