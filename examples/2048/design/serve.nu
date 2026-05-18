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
use ../tfe/render.nu *
use ../tfe/game.nu *

const HERE = path self | path dirname

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
  {slug: "markdown"   title: "markdown"   desc: "the full set of markdown the /notes pages render: headings, prose, lists, links, code, quotes."}
]

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
        (kbd-btn "play now" --variant primary --href "/new")
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
def design-page [current: string]: nothing -> record {
  let stories = render-stories $current
  let entry = $CATALOG | where slug == $current | first
  # Build a JS array literal with single-quoted strings -- avoids the
  # double-quotes in to-json clobbering the outer $"..." interpolation.
  let slugs_js = "[" + ($CATALOG | get slug | each {|s| $"'($s)'"} | str join ", ") + "]"
  (HTML
    (HEAD
      (META {charset: "UTF-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE $"nu2048 / design / ($entry.title)")
      (LINK {rel: "preconnect" href: "https://fonts.googleapis.com"})
      (LINK {rel: "preconnect" href: "https://fonts.gstatic.com" crossorigin: true})
      (LINK {rel: "stylesheet" href: "https://fonts.googleapis.com/css2?family=Source+Code+Pro:wght@400;700&family=Source+Sans+3:wght@400;700&display=swap"})
      (LINK {rel: "stylesheet" href: "../styles.css"})
      (LINK {rel: "stylesheet" href: "./design.css"}))
    (BODY {class: "design" "data-slug": $current}
      # Browser-relative hrefs: we're at /design/<slug>, so ../.. -> /,
      # ../ -> /design, and the current slug self-links.
      (breadcrumb
        --left [
          (A {href: "../../"} "nu2048")
          (SPAN {class: "sep"} "·")
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
          ...$stories))
      (SCRIPT {} {__html: ($"const slugs = ($slugs_js);\n" + (r#'const current = document.body.dataset.slug;
document.addEventListener('keydown', e => {
  if (!e.ctrlKey || e.metaKey || e.altKey) return;
  const i = slugs.indexOf(current);
  if (i < 0) return;
  if (e.key === 'n') { location.href = slugs[(i + 1) % slugs.length]; e.preventDefault(); }
  if (e.key === 'p') { location.href = slugs[(i - 1 + slugs.length) % slugs.length]; e.preventDefault(); }
});'#))})))
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
        design-page $ctx.slug
      }
    })
  ]
}
