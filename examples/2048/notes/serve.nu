# nu2048 notes sub-site: digital-garden style pages, each h1 in a
# content/*.md file becomes its own page. Authors keep related topics
# in one file (easier to edit, reorder, cross-reference); the runtime
# splits on h1 boundaries so each section gets its own URL.
#
# Mount under /notes from the parent serve.nu:
#   let notes = source notes/serve.nu
#   ...
#   (mount "/notes" $notes)
#
# Pages link to each other via in-doc markdown links the author writes;
# no auto-generated next/prev. Wandering is intentional.

use http-nu/router *
use http-nu/html *
use http-nu/datastar *
use ./pages.nu *
use ../tfe/render.nu *

const HERE = path self | path dirname
const CONTENT = $HERE | path join "content"

# Per-server-start cache-buster, mirrors the parent serve.nu pattern.
let REV = random uuid | str substring 0..7

# Every section across every .md in content/. Single concatenated index.
def all-pages []: nothing -> list {
  ls $CONTENT | where {|f| ($f.name | str ends-with ".md")} | each {|f|
    split-md $f.name
  } | flatten
}

{|req|
  dispatch $req [
    (route {method: GET path: "/"} {|req ctx|
      # The shared layout's asset URLs (styles, script, ellie, splash,
      # my-games, design) live at the parent app's root, NOT under the
      # /notes sub-mount. Strip just the /notes segment from
      # mount_prefix so $req|href resolves to the parent app's root --
      # whether that root is "" (standalone) or "/2048" (hub mount).
      # Body links keep $req with the full prefix for sub-site links.
      let root_req = $req | upsert mount_prefix ($req.mount_prefix | str replace -r '/notes$' '')
      let pages = all-pages
      ([
        (DIV {class: "page"}
          (H1 "notes")
          (P "a wandering set of pages about 2048 -- the game, its history, and how this implementation works.")
          (UL ($pages | each {|p|
            (LI (A {href: ($req | href $"/($p.slug)")} $p.title))
          })))
      ] | layout $root_req $REV $DATASTAR_JS_PATH
            --title "nu2048 / notes"
            --body-class "notes")
    })

    (route {method: GET path-matches: "/:slug"} {|req ctx|
      let page = all-pages | where slug == $ctx.slug | first
      if $page == null {
        "Not Found" | metadata set { merge {'http.response': {status: 404}} }
      } else {
        let root_req = $req | upsert mount_prefix ""
        let rendered = $page.body | .md | get __html
        ([
          (DIV {class: "page"}
            (H1 $page.title)
            (DIV {class: "prose"} {__html: $rendered}))
        ] | layout $root_req $REV $DATASTAR_JS_PATH
              --title $"nu2048 / ($page.title)"
              --body-class "notes")
      }
    })
  ]
}
