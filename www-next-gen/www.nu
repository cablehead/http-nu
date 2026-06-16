# www.nu - prototype of the http-nu site redone on stellar variables.
#
#   - typography and layout come entirely from assets/stellar.css +
#     assets/base.css (semantic elements, a few utility classes)
#   - the docs section is the project README split into navigable pages
#     by readme.nu, so the README stays the single source of truth
#
# Run: http-nu --datastar :3001 www.nu
# (regenerate assets/stellar.css with: stellar gen, stellar.key in place)

const script_dir = path self | path dirname

use http-nu/router *
use http-nu/datastar *
use http-nu/html *
use readme.nu *

# README is read once at handler load. It lives one level up (this dir sits at
# the repo root as www-next-gen/), and stays the single source of truth.
const readme_path = path self | path dirname | path join .. README.md
let readme = (open --raw $readme_path | decode utf-8)

# Section groupings: these heading slugs hold child pages rather than
# becoming one giant page.
let groups = [reference embedded-modules templates]
let pages = ($readme | pages $groups)
let anchors = ($readme | anchor-map $groups)
let titles = ($readme | headings | reduce --fold {} {|h, acc| $acc | upsert $h.slug $h.title })

def icon [name: string] {
  {__html: $"<iconify-icon icon=\"($name)\" noobserver></iconify-icon>"}
}

def page-head [title: string] {
  (HEAD
    (META {charset: "utf-8"})
    (META {name: "viewport" content: "width=device-width, initial-scale=1"})
    (META {name: "view-transition" content: "same-origin"})
    (TITLE $title)
    # stellar props -> base (raw tags + generic utils/components) -> brand layer
    (LINK {rel: "stylesheet" href: "/assets/stellar.css"})
    (LINK {rel: "stylesheet" href: "/assets/base.css"})
    (LINK {rel: "stylesheet" href: "/assets/brand.css"})
    (SCRIPT {src: "https://cdn.jsdelivr.net/npm/iconify-icon@2/dist/iconify-icon.min.js"})
    (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
    # theme: restore before paint to avoid a flash
    (SCRIPT {__html: r#'
(function() {
  var saved = localStorage.getItem('theme');
  var dark = saved ? saved === 'dark' : matchMedia('(prefers-color-scheme:dark)').matches;
  if (dark) document.documentElement.classList.add('dark');
})();
'#})
  )
}

def theme-toggle [] {
  (BUTTON {
    id: "theme-toggle"
    title: "Toggle theme"
    onclick: "var d=document.documentElement.classList.toggle('dark');localStorage.setItem('theme',d?'dark':'light')"
  } (icon "lucide:sun-moon"))
}

# Shared nav: brand + links + theme toggle, on every page.
def nav-bar [] {
  (NAV {class: "nav"}
    (DIV {class: "brand"} (A {href: "/"} "http-nu"))
    (DIV {class: "links"}
      (A {href: "/docs"} "Docs")
      (A {href: "https://github.com/cablehead/http-nu"} "GitHub")
      (A {href: "https://discord.com/invite/YNbScHBHrh"} "Discord")
      (theme-toggle)))
}

# Inject copy buttons into rendered code blocks.
def inject-copy-btns []: record -> record {
  let btn = '<button class="copy-btn" title="Copy"><iconify-icon icon="lucide:copy" width="16" height="16"></iconify-icon></button>'
  {__html: (
    $in.__html
    | str replace --all '<pre>' $'<div class="code-block">($btn)<pre>'
    | str replace --all '</pre>' '</pre></div>'
  )}
}

def copy-script [] {
  (SCRIPT {__html: r#'
document.addEventListener('click', function(e) {
  var btn = e.target.closest('.copy-btn');
  if (!btn) return;
  var code = btn.parentElement.querySelector('pre code');
  if (!code) return;
  navigator.clipboard.writeText(code.textContent);
  var icon = btn.querySelector('iconify-icon');
  if (icon) { icon.setAttribute('icon','lucide:check'); setTimeout(function(){icon.setAttribute('icon','lucide:copy')},800); }
});
'#})
}

# --- docs navigation -------------------------------------------------

# Sidebar / index nav: pages in document order, with a group label
# emitted whenever the group changes.
def docs-nav [current: string] {
  let items = ($pages | reduce --fold {seen: [], html: []} {|p, acc|
    let label = (if $p.group != null and ($p.group not-in $acc.seen) {
      [(SPAN {class: "toc-group"} ($titles | get $p.group))]
    } else { [] })
    {
      seen: (if $p.group == null { $acc.seen } else { $acc.seen | append $p.group })
      html: ($acc.html | append $label | append (
        LI (A {
          href: $"/docs/($p.slug)"
          class: (if $p.slug == $current { "active" } else { "" })
        } $p.title)
      ))
    }
  })
  (UL ($items.html))
}

# --- routes ----------------------------------------------------------

# --- splash hero (branded, www palette via stellar named colors) ---

def svg-top [] {
  (SVG {viewBox: "0 0 600 70" xmlns: "http://www.w3.org/2000/svg" class: "curve"}
    (PATH {d: "M 20,65 Q 70,45 100,38" stroke: "currentColor" stroke-width: "1" fill: "none"})
    (PATH {id: "curve-top" d: "M 100,38 Q 300,15 500,38" fill: "none"})
    (TEXT {fill: "currentColor" font-family: "Georgia, serif" font-style: "italic" font-size: "28px"}
      (TEXTPATH {href: "#curve-top" startOffset: "50%" text-anchor: "middle"} "The surprisingly performant"))
    (PATH {d: "M 500,38 Q 530,45 580,65" stroke: "currentColor" stroke-width: "1" fill: "none"}))
}

def svg-bottom [] {
  (SVG {viewBox: "0 0 600 70" xmlns: "http://www.w3.org/2000/svg" class: "curve"}
    (PATH {id: "curve-bottom" d: "M 50,15 Q 300,65 550,15" fill: "none"})
    (TEXT {fill: "currentColor" font-family: "Georgia, serif" font-style: "italic" font-size: "28px"}
      (TEXTPATH {href: "#curve-bottom" startOffset: "50%" text-anchor: "middle"} "that fits in your back pocket")))
}

def wave-divider [] {
  (SVG {class: "wave" viewBox: "0 0 1200 150" preserveAspectRatio: "none" xmlns: "http://www.w3.org/2000/svg"}
    (PATH {d: "M0,50 Q300,150 600,50 T1200,50 L1200,150 L0,150 Z" fill: "var(--named-green-0)"})
    (PATH {d: "M0,80 Q300,0 600,80 T1200,80 L1200,150 L0,150 Z" fill: "var(--named-orange-0)"}))
}

def splash-hero [] {
  (DIV {class: "splash-hero"}
    (nav-bar)
    (DIV {class: "taglines"}
      (svg-top)
      (IMG {src: "/assets/ellie.png" alt: "http-nu mascot"})
      (DIV {class: "badges"}
        (SPAN {class: "badge tone-orange"} (A {href: "https://www.nushell.sh"} "Nushell") "-scriptable!")
        (SPAN {class: "badge tone-grape"}
          (A {href: "https://cross.stream"} "cross." (SPAN {class: "stream"} "stream")) "-powered")
        (SPAN {class: "badge tone-red"} (A {href: "https://data-star.dev"} "Datastar") "-ready"))
      (SPAN {class: "badge tone-green upper mx-auto"} "HTTP Server")
      (svg-bottom))
    (wave-divider))
}

# A section heading that links to its own anchor (like ./www), so each
# section is addressable. Extra children (e.g. a gif) sit after the link.
def section-head [title: string ...extra] {
  let slug = ($title | str downcase | str replace --all ' ' '-')
  (H2 {id: $slug} (A {href: $"#($slug)"} $title) ...$extra)
}

# "Give it a try": install method tabs (Datastar $tab signal) over a terminal
# that shows the chosen install command, then the hello-world run.
def give-it-a-try [] {
  let methods = [
    [brew "Homebrew" "brew install cablehead/tap/http-nu"]
    [cargo "Cargo" "cargo install --locked http-nu"]
    [eget "eget" "eget cablehead/http-nu"]
    [nix "Nix" "nix-shell -p http-nu"]
  ]
  (DIV {"data-signals:tab": "'brew'"}
    (section-head "Give it a try"
      (IMG {class: "rocket" alt: "" src: "https://data-star.dev/cdn-cgi/image/format=auto,width=96/static/images/rocket-animated-1d781383a0d7cbb1eb575806abeec107c8a915806fb55ee19e4e33e8632c75e5.gif"}))
    (DIV {class: "terminal"}
      (DIV {class: "terminal-bar"}
        (SPAN {class: "terminal-dots"} (SPAN) (SPAN) (SPAN))
        ($methods | each {|m|
          BUTTON {class: "terminal-tab" "data-class:is-active": $"$tab === '($m.0)'" "data-on:click": $"$tab = '($m.0)'"} $m.1
        }))
      (DIV {class: "terminal-body"}
        ($methods | each {|m|
          DIV {"data-show": $"$tab === '($m.0)'"} (SPAN {class: "prompt"} "$ ") $m.2
        })
        (DIV (SPAN {class: "prompt"} "$ ") "http-nu :3001 -c '{|req| \"Hello world\"}'")
        (DIV (SPAN {class: "prompt"} "$ ") "curl -s localhost:3001")
        (DIV {class: "out"} "Hello world"))))
}

{|req|
  dispatch $req [

    # landing page
    (route {method: GET path: "/"} {|req ctx|
      (HTML
        (page-head "http-nu")
        (BODY
          (splash-hero)
          (MAIN {class: "container"}
            (give-it-a-try)

            (P {class: "center"} (STRONG (A {href: "/docs"} "Read the docs ->")))

            (section-head "Why http-nu")
            (DIV {class: "grid"}
              (DIV {class: "card"} (H3 (icon "lucide:feather") " Tiny") (P "A single binary. Hand it a Nushell closure and you have a server."))
              (DIV {class: "card"} (H3 (icon "lucide:zap") " Fast") (P "Streaming responses, SSE, and HTTP/2 over TLS out of the box."))
              (DIV {class: "card"} (H3 (icon "lucide:boxes") " Batteries") (P "Routing, an HTML DSL, templates, cookies, and a Datastar SDK, all embedded."))
              (DIV {class: "card"} (H3 (icon "lucide:database") " Stateful") (P "In-memory SQLite, a local bus, and an embedded cross.stream event store.")))
          )
          (copy-script)
        )
      )
    })

    # docs index
    (route {method: GET path: "/docs"} {|req ctx|
      (HTML
        (page-head "Docs - http-nu")
        (BODY
          (nav-bar)
          (MAIN {class: "container"}
            (DIV {class: "prose"}
              (H1 "Documentation")
              (P {class: "muted"} "Generated from the project "
                (A {href: "https://github.com/cablehead/http-nu/blob/main/README.md"} "README")
                ", split into pages by section."))
            (DIV {class: "toc"}
              (docs-nav "")))
          (copy-script)
        )
      )
    })

    # docs page
    (route {path-matches: "/docs/:slug"} {|req ctx|
      let slug = $ctx.slug
      let idx = ($pages | enumerate | where item.slug == $slug | get index.0? | default null)
      if $idx == null {
        ("Not Found" | metadata set { merge {'http.response': {status: 404}} })
      } else {
        let page = ($pages | get $idx)
        let prev = (if $idx > 0 { $pages | get ($idx - 1) } else { null })
        let next = ($pages | get -o ($idx + 1))
        let content = ($readme | render-page $page $anchors | inject-copy-btns)
        (HTML
          (page-head $"($page.title) - http-nu")
          (BODY
            (nav-bar)
            (MAIN {class: "container with-sidebar"}
              (ASIDE {class: "toc"} (docs-nav $slug))
              (ARTICLE {class: "prose"}
                $content
                (NAV {class: "pager"}
                  (SPAN (if ($idx > 0) { A {href: $"/docs/($prev.slug)"} $"<- ($prev.title)" }))
                  (SPAN (if ($next != null) { A {href: $"/docs/($next.slug)"} $"($next.title) ->" })))))
            (copy-script)
          )
        )
      }
    })

    # static assets (stellar.css, base.css, images)
    (route {path-matches: "/assets/:file"} {|req ctx|
      .static ($script_dir | path join "assets") $ctx.file
    })
  ]
}
