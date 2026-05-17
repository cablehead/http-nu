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

# Catalog: every component on the site, in nav order. Order = the
# sequence Ctrl-N / Ctrl-P walks through.
const CATALOG = [
  {slug: "kbd-btn"    title: "kbd-btn"    desc: "bracketed key-cap button: [ h ]. triggers a move or navigation."}
  {slug: "breadcrumb" title: "breadcrumb" desc: "header nav row. left = path crumbs, right = action shortcuts."}
  {slug: "board"      title: "board"      desc: "4x4 game grid. tiles, ghosts, dim mask, max-tile highlight."}
]

# Render one or more stories (sample invocations) for a slug. Returns a
# list of HTML DSL records to drop into the right column.
def render-stories [slug: string]: nothing -> list {
  match $slug {
    "kbd-btn" => [
      (story "move key (triggers move via [data-intent])" [
        (kbd-btn "h" --intent "h")
        (kbd-btn "j" --intent "j")
        (kbd-btn "k" --intent "k")
        (kbd-btn "l" --intent "l")
      ])
      (story "navigation shortcut (links via [data-href])" [
        (kbd-btn "esc" --href "/")
        (kbd-btn "n" --href "/new")
      ])
      (story "bracketless (the fx toggle -- not a keypress)" [
        (kbd-btn "fx" --bracketless)
      ])
    ]
    "breadcrumb" => [
      (story "splash header: page title + action" [
        (breadcrumb
          --left [(A {href: "/"} "past games")]
          --right [
            (A {href: "/new"} "new game")
            (kbd-btn "n" --href "/new")
          ])
      ])
      (story "/play header: path with shortcut + game-id" [
        (breadcrumb
          --left [
            (A {href: "/"} "past games")
            (kbd-btn "esc" --href "/")
            (SPAN {class: "sep"} "·")
            (A {href: "/play/03g5xxxx" class: "game-id"} "03g5xxxx")
          ]
          --right [
            (A {href: "/new"} "new game")
            (kbd-btn "n" --href "/new")
          ])
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
