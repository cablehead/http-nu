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

const HERE = path self | path dirname
const CONTENT = $HERE | path join "content"

# Slugify a heading: lowercase, non-alphanumeric runs become single
# hyphens, trim leading/trailing hyphens.
def slugify [s: string]: nothing -> string {
  $s | str downcase | str replace -ar '[^a-z0-9]+' '-' | str trim --char '-'
}

# Split a markdown file on h1 boundaries. Returns [{slug, title, body}].
def split-md [path: string]: nothing -> list {
  open $path
  | split row "\n# "
  | skip 1
  | each {|sec|
      let lines = $sec | lines
      let title = $lines | first
      {
        slug: (slugify $title)
        title: $title
        body: ($lines | skip 1 | str join "\n")
      }
    }
}

# Every section across every .md in content/. Single concatenated index.
def all-pages []: nothing -> list {
  ls $CONTENT | where {|f| ($f.name | str ends-with ".md")} | each {|f|
    split-md $f.name
  } | flatten
}

# Minimal page chrome. Links to the parent site's styles via a
# relative-up path so the same markup works whether the site is
# deployed standalone (nu2048.com) or mounted under another prefix
# (the examples hub puts 2048 at /2048).
def notes-layout [title: string, body_children: list]: nothing -> record {
  (HTML
    (HEAD
      (META {charset: "UTF-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE $title)
      (LINK {rel: "preconnect" href: "https://fonts.googleapis.com"})
      (LINK {rel: "preconnect" href: "https://fonts.gstatic.com" crossorigin: true})
      (LINK {rel: "stylesheet" href: "https://fonts.googleapis.com/css2?family=Source+Code+Pro:wght@400;700&family=Source+Sans+3:wght@400;700&display=swap"})
      (LINK {rel: "stylesheet" href: "../styles.css"}))
    (BODY {class: "notes"}
      (MAIN {class: "page"} ...$body_children)))
}

{|req|
  dispatch $req [
    (route {method: GET path: "/"} {|req ctx|
      let pages = all-pages
      (notes-layout "nu2048 notes" [
        (H1 "notes")
        (P "a wandering set of pages about 2048 -- the game, its history, and how this implementation works.")
        (UL ($pages | each {|p|
          (LI (A {href: ($req | href $"/($p.slug)")} $p.title))
        }))
      ])
    })

    (route {method: GET path-matches: "/:slug"} {|req ctx|
      let page = all-pages | where slug == $ctx.slug | first
      if $page == null {
        "Not Found" | metadata set { merge {'http.response': {status: 404}} }
      } else {
        let rendered = $page.body | .md | get __html
        (notes-layout $"nu2048 - ($page.title)" [
          (H1 $page.title)
          {__html: $rendered}
        ])
      }
    })
  ]
}
