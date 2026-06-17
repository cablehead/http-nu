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
let groups = []
let pages = ($readme | pages $groups)
let anchors = ($readme | anchor-map $groups)
let titles = ($readme | headings | reduce --fold {} {|h, acc| $acc | upsert $h.slug $h.title })

# Precompute each page's h3 sub-sections at load (parsing the README at request
# time, deep in the DSL tree, overflows the stack). Map: page slug -> sections.
let all_headings = ($readme | headings)
let page_secs = ($pages | reduce --fold {} {|p, acc|
  $acc | upsert $p.slug ($all_headings | where line > $p.start and line <= $p.end and level == ($p.level + 1) | select slug title)
})

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

# Sidebar nav: pages in order; the active page expands to its sections.
# Build the list in the body (outer lets are in scope here); pass the finished
# list to UL - a closure passed to the DSL evaluates in a scope without them.
def docs-nav [current: string] {
  let items = ($pages | each {|p|
    let active = ($p.slug == $current)
    let sub = (if $active {
      let secs = ($page_secs | get $p.slug)
      if ($secs | is-empty) { [] } else {
        [(UL ($secs | each {|s| LI (A {href: $"/docs/($p.slug)#($s.slug)"} $s.title)}))]
      }
    } else { [] })
    (LI (A {href: $"/docs/($p.slug)" class: (if $active { "active" } else { "" })} $p.title) ...$sub)
  })
  (UL $items)
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

# --- topic hubs ------------------------------------------------------
# A hub weaves one theme together: the reference doc, the worked example
# broken into liftable parts (source shown for each), and how to run it.
# Pilot covers Templates; the shape generalizes to every theme.

# A code "toy": a highlighted source fragment you can lift out of an example
# and read on its own, with a copy button. (Editable/live is a later step;
# for now the source is shown clearly.) Reuses the docs .code-block chrome.
def code-toy [src: string, lang: string] {
  let btn = '<button class="copy-btn" title="Copy"><iconify-icon icon="lucide:copy" width="16" height="16"></iconify-icon></button>'
  (DIV {class: "code-block"} {__html: $btn} (PRE (CODE ($src | str trim | .highlight $lang))))
}

# A labelled toy: a heading, a one-line gloss, and the source.
def toy [title: string, explain: string, src: string, lang: string] {
  (SECTION {class: "toy"}
    (H3 $title)
    (P {class: "muted"} $explain)
    (code-toy $src $lang))
}

def hub-page [title: string, body] {
  (HTML
    (page-head $"($title) - http-nu")
    (BODY
      (nav-bar)
      (MAIN {class: "container"} $body)
      (copy-script)))
}

def templates-hub [] {
  let inline_src = r#'
# index route: inline mode renders a self-contained snippet
_ => {
  {} | .mj --inline '<h1>Templates</h1>
<p>This page is rendered with <code>.mj --inline</code>.</p>'
}'#
  let file_src = r#'
# /file: render page.html from disk. {% extends %} and {% include %}
# resolve from the template dir and subdirs only (no ../, no absolute paths)
"/file" => { {name: "World"} | .mj ($templates_dir | path join page.html) }'#
  let files_src = r#'<!-- page.html: extends a base, includes a partial, fills a slot -->
{% extends "base.html" %}
{% block title %}My Page{% endblock %}
{% block content %}
{% include "nav.html" %}
<main>Hello {{ name }}</main>
{% endblock %}

<!-- base.html: the shell -->
<head><title>{% block title %}Default{% endblock %}</title></head>
<body>{% block content %}{% endblock %}</body>

<!-- nav.html: the partial -->
<nav><a href="./">Home</a> | Page</nav>'#
  let topic_src = r#'
# seed templates into the store as topics, once at startup
if $HTTP_NU.store != null {
  open (topics/page.html) | .append page.html
  open (topics/base.html) | .append base.html
  open (topics/nav.html) | .append nav.html
}

# /topic: same render, but template names resolve as cross.stream topics
"/topic" => { {name: "World"} | .mj --topic "page.html" }'#
  let run_src = r#'
# run with a store so the /topic route can resolve its templates
http-nu :3001 --store ./store examples/templates/serve.nu

#   /        index   - rendered with .mj --inline
#   /file    disk    - {% extends %}/{% include %} from the template dir
#   /topic   store   - templates resolved as cross.stream topics'#
  (ARTICLE
    (P {class: "muted"} (A {href: "/docs"} "Docs") " / Hubs / Templates")
    (H1 "Templates")
    (P "Render a page from data with "
      (A {href: "https://github.com/mitsuhiko/minijinja"} "minijinja")
      " (Jinja2-compatible) templates. The same render runs from three sources: "
      "an inline snippet, files on disk, or templates kept in the event store.")
    (A {class: "card panel" href: "/docs/templates--output"}
      (H3 (icon "lucide:book-open") " Reference: Templates & output")
      (P {class: "muted"} "The .mj command, compile / render, syntax highlighting, and Markdown."))
    (H2 "Worked example")
    (P "The "
      (A {href: "https://github.com/cablehead/http-nu/tree/main/examples/templates"} "templates example")
      ", one part per source. Lift any piece and read it on its own.")
    (toy "Inline mode" "Self-contained: no file or store lookups, so extends and include are not available." $inline_src "nu")
    (toy "File mode (disk)" "Renders a template from disk; extends and include resolve from the template's directory." $file_src "nu")
    (toy "The files it resolves" "page.html extends base.html and includes nav.html, all from the same directory." $files_src "html")
    (toy "Topic mode (store)" "With --store, template names resolve as cross.stream topics, so templates live in the event store, not on disk." $topic_src "nu")
    (H2 "Run it")
    (P "Start the server with a store so the topic route can resolve its templates:")
    (code-toy $run_src "bash")
    (NAV {class: "pager"}
      (A {class: "pager-link panel pager-prev" href: "/docs/templates--output"}
        (SPAN {class: "pager-dir"} (icon "lucide:arrow-left") "Reference")
        (SPAN {class: "pager-page"} "Templates & output"))
      (A {class: "pager-link panel pager-next" href: "/docs"}
        (SPAN {class: "pager-dir"} "All topics" (icon "lucide:arrow-right"))
        (SPAN {class: "pager-page"} "Browse the docs"))))
}

def streaming-hub [] {
  let chunks_src = r#'
# values from a streaming pipeline flush to the client as they are produced,
# no waiting for the whole response to be ready
generate {|_|
  sleep 1sec
  {out: $"(date now | to text)\n" next: true}
} true'#
  let to_sse_src = r#'
# to sse formats {data? id? event? retry?} records for text/event-stream,
# and sets content-type: text/event-stream for you
{data: 'hello'} | to sse
# data: hello

{id: 1 event: greet data: 'hi'} | to sse
# id: 1
# event: greet
# data: hi'#
  let page_src = r#'
# the page: a count signal, and buttons that POST to routes.
# data-text binds the signal into the DOM; data-on:click posts.
(BODY {"data-signals": "{count: 0}"}
  (P "Count: " (SPAN {"data-text": "$count"} "0"))
  (BUTTON {"data-on:click": "@post('./increment')"} "Increment")
  (DIV {id: "time"} "--:--:--.---")
  (BUTTON {"data-on:click": "@post('./time')"} "Get Time"))'#
  let patch_signals_src = r#'
# /increment: read signals, bump count, stream a signal patch back as SSE
(route {method: POST path: "/increment"} {|req ctx|
  let signals = from datastar-signals $req
  let count = ($signals.count? | default 0) + 1
  {count: $count} | to datastar-patch-signals | to sse
})'#
  let patch_elements_src = r#'
# /time: stream an element; Datastar swaps it in by matching id
(route {method: POST path: "/time"} {|req ctx|
  let time = date now | format date "%H:%M:%S%.3f"
  DIV {id: "time"} $time | to datastar-patch-elements | to sse
})'#
  let execute_script_src = r#'
# /hello: stream a script for the client to run
(route {method: POST path: "/hello"} {|req ctx|
  "alert('Hello from the server!')" | to datastar-execute-script | to sse
})'#
  let run_src = r#'
# serve the example with the embedded Datastar bundle
http-nu --datastar :3001 examples/datastar-sdk/serve.nu

# click a button: the browser POSTs, the server streams an SSE patch back'#
  (ARTICLE
    (P {class: "muted"} (A {href: "/docs"} "Docs") " / Hubs / Streaming")
    (H1 "Streaming & events")
    (P "Send chunks as they are produced, and push server-sent events. A streaming "
      "pipeline's values flush to the client immediately; "
      (CODE "to sse") " formats records for the event stream.")
    (A {class: "card panel" href: "/docs/streaming--events"}
      (H3 (icon "lucide:book-open") " Reference: Streaming & events")
      (P {class: "muted"} "Streaming responses, the to sse command, and streaming input."))
    (H2 "Streaming responses")
    (toy "Chunks as they are produced" "A streaming pipeline like generate flushes each value to the client as an HTTP chunk, with no buffering." $chunks_src "nu")
    (H2 "Server-sent events")
    (toy "to sse" "Formats {data? id? event? retry?} records for text/event-stream, and sets the content-type automatically." $to_sse_src "nu")
    (H2 "Worked example")
    (P "The "
      (A {href: "https://github.com/cablehead/http-nu/tree/main/examples/datastar-sdk"} "datastar-sdk example")
      ": a button POSTs, the server streams an SSE patch back. Three patch kinds, one per button.")
    (toy "The page (client)" "A count signal and buttons that POST to routes; data-text binds the signal into the DOM." $page_src "nu")
    (toy "Patch a signal" "Read signals from the request, change one, stream a signal patch back." $patch_signals_src "nu")
    (toy "Patch the DOM" "Stream an element and Datastar swaps it in by matching id." $patch_elements_src "nu")
    (toy "Run a script" "Stream a script for the client to execute." $execute_script_src "nu")
    (H2 "Run it")
    (P "Serve the example with the embedded Datastar bundle:")
    (code-toy $run_src "bash")
    (NAV {class: "pager"}
      (A {class: "pager-link panel pager-prev" href: "/docs/streaming--events"}
        (SPAN {class: "pager-dir"} (icon "lucide:arrow-left") "Reference")
        (SPAN {class: "pager-page"} "Streaming & events"))
      (A {class: "pager-link panel pager-next" href: "/docs"}
        (SPAN {class: "pager-dir"} "All topics" (icon "lucide:arrow-right"))
        (SPAN {class: "pager-page"} "Browse the docs"))))
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
              (DIV {class: "card panel"} (H3 (icon "lucide:feather") " Tiny") (P "A single binary. Hand it a Nushell closure and you have a server."))
              (A {class: "card panel" href: "/hub/streaming"} (H3 (icon "lucide:zap") " Fast") (P "Streaming responses, SSE, and HTTP/2 over TLS out of the box."))
              (A {class: "card panel" href: "/hub/templates"} (H3 (icon "lucide:boxes") " Batteries") (P "Routing, an HTML DSL, templates, cookies, and a Datastar SDK, all embedded."))
              (DIV {class: "card panel"} (H3 (icon "lucide:database") " Stateful") (P "In-memory SQLite, a local bus, and an embedded cross.stream event store.")))
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
            (ARTICLE
              (H1 "Documentation")
              (P {class: "muted"} "The project "
                (A {href: "https://github.com/cablehead/http-nu/blob/main/README.md"} "README")
                ", split into pages. Pick a topic:"))
            (DIV {class: "grid"}
              ($pages | each {|p|
                let secs = ($page_secs | get $p.slug)
                (A {class: "card panel" href: $"/docs/($p.slug)"}
                  (H3 $p.title)
                  (if ($secs | is-empty) { "" } else { (SMALL ($secs | get title | str join " \u{b7} ")) }))
              })))
          (copy-script)
        )
      )
    })

    # topic hubs (weave a theme: reference doc + worked-example toys + run it)
    (route {path-matches: "/hub/:slug"} {|req ctx|
      match $ctx.slug {
        "templates" => (hub-page "Templates" (templates-hub))
        "streaming" => (hub-page "Streaming & events" (streaming-hub))
        _ => ("Not Found" | metadata set { merge {'http.response': {status: 404}} })
      }
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
              (DIV {class: "docs-menu" "data-signals:nav": "false" "data-class:open": "$nav"}
                (BUTTON {class: "docs-toggle" "data-on:click": "$nav = !$nav"} "Pages")
                (NAV {class: "docs-side toc"} (docs-nav $slug)))
              (ARTICLE
                $content
                (NAV {class: "pager"}
                  (if ($idx > 0) {
                    (A {class: "pager-link panel pager-prev" href: $"/docs/($prev.slug)"}
                      (SPAN {class: "pager-dir"} (icon "lucide:arrow-left") "Previous")
                      (SPAN {class: "pager-page"} $prev.title))
                  } else { "" })
                  (if ($next != null) {
                    (A {class: "pager-link panel pager-next" href: $"/docs/($next.slug)"}
                      (SPAN {class: "pager-dir"} "Next" (icon "lucide:arrow-right"))
                      (SPAN {class: "pager-page"} $next.title))
                  } else { "" }))))
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
