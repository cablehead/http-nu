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
      (A {href: "/themes"} "Themes")
      (A {href: "/reference"} "Reference")
      (A {href: "/how-tos"} "How-tos")
      (A {href: "https://github.com/cablehead/http-nu"} "GitHub")
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

def templates-files-section [] {
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
  (DIV
    (H2 "Files and the store")
    (P "The snippet above is self-contained. Point "
      (CODE ".mj") " at a file and it resolves "
      (CODE "{% extends %}") " / " (CODE "{% include %}")
      " from disk; point it at the store and the same names resolve as cross.stream topics.")
    (toy "From a file" "extends and include resolve from the template's directory and subdirs only." $file_src "nu")
    (toy "The files it resolves" "page.html extends base.html and includes nav.html, all from the same directory." $files_src "html")
    (toy "From the store" "Seed the templates as topics once, then resolve them by name, with nothing on disk." $topic_src "nu")
    (P (A {href: "/themes/templates/reference"} "Full reference: modes, compile / render, highlighting, Markdown ->")))
}

def streaming-overview [] {
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
    (P "Send chunks as they are produced, and push server-sent events. A streaming "
      "pipeline's values flush to the client immediately; "
      (CODE "to sse") " formats records for the event stream.")
    (DIV {class: "playground" "data-signals": "{count: 0}"}
      (P {class: "muted"} "Click a button. The browser POSTs and the server streams an SSE patch back, live:")
      (DIV {class: "pg-live"}
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/streaming/increment')"} "Increment")
        (SPAN "count " (SPAN {class: "pg-val" "data-text": "$count"} "0"))
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/streaming/time')"} "Get time")
        (SPAN "time " (SPAN {class: "pg-val" id: "stream-time"} "--:--:--"))))
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
    (code-toy $run_src "bash"))
}

# --- theme namespace -------------------------------------------------
# /themes/<theme> is a theme: an overview that leads with something to poke,
# with a reference facet (a slice of the canonical /reference) one click away.
# The shell + facet nav generalize to every theme.

def theme-facets [theme: string, current: string] {
  let facets = [[slug, name]; [overview, Overview] [reference, Reference]]
  (NAV {class: "facets"}
    ($facets | each {|f|
      let href = (if $f.slug == "overview" { $"/themes/($theme)" } else { $"/themes/($theme)/($f.slug)" })
      (A {href: $href class: (if $f.slug == $current { "active" } else { "" })} $f.name)
    }))
}

# Breadcrumb trail of ancestor links, separated by " / ". The current page is the
# H1, not part of the trail. One scheme for every page that has ancestors.
def crumbs [trail: list] {
  let parts = ($trail | enumerate | each {|it|
    let link = (A {href: ($it.item | get 1)} ($it.item | get 0))
    if $it.index > 0 { [" / " $link] } else { [$link] }
  } | flatten)
  (P {class: "muted crumbs"} ...$parts)
}

def theme-shell [theme: string, title: string, current: string, body] {
  let trail = (if $current == "overview" {
    [["Themes" "/themes"]]
  } else {
    [["Themes" "/themes"] [$title $"/themes/($theme)"]]
  })
  (HTML
    (page-head $"($title) - http-nu")
    (BODY
      (nav-bar)
      (MAIN {class: "container"}
        (crumbs $trail)
        (H1 $title)
        (theme-facets $theme $current)
        $body)
      (copy-script)))
}

# The playground render: a JSON context record through an inline template.
# Valid output is injected as HTML (the point); errors show as plain text.
def tpl-render [tpl: string, data: string] {
  let ctx = (try { $data | from json } catch { {} })
  let res = (try { {ok: ($ctx | .mj --inline $tpl)} } catch {|e| {err: $e.msg} })
  if ($res.err? != null) {
    (DIV {id: "tpl-out" class: "tpl-out is-err"} $res.err)
  } else {
    (DIV {id: "tpl-out" class: "tpl-out"} {__html: $res.ok})
  }
}

# Preset snippets the chips load into the playground.
def tpl-preset [name: string] {
  match $name {
    "loop" => {tpl: "<ul>{% for i in items %}<li>{{ i }}</li>{% endfor %}</ul>", data: '{"items": ["a", "b", "c"]}'}
    "cond" => {tpl: "{% if vip %}VIP {{ name }}{% else %}hi {{ name }}{% endif %}", data: '{"name": "Sam", "vip": true}'}
    "filter" => {tpl: "{{ name | upper }} has {{ name | length }} letters", data: '{"name": "ada"}'}
    _ => {tpl: "Hello {{ name }}!", data: '{"name": "world"}'}
  }
}

def templates-overview [] {
  let def_tpl = "Hello {{ name }}!"
  let def_data = '{"name": "world"}'
  (ARTICLE
    (P "Render a page from data. The same render runs from three sources: an inline "
      "snippet, files on disk, or templates kept in the event store. Poke at it:")
    (DIV {class: "playground"}
      (DIV {class: "pg-inputs"}
        (DIV {class: "pg-field"}
          (LABEL "template")
          (TEXTAREA {"data-bind:tpl": true "data-on:input__debounce.300ms": "@post('/themes/templates/render')" rows: "3" spellcheck: "false"} $def_tpl))
        (DIV {class: "pg-field"}
          (LABEL "data (JSON)")
          (TEXTAREA {"data-bind:data": true "data-on:input__debounce.300ms": "@post('/themes/templates/render')" rows: "3" spellcheck: "false"} $def_data)))
      (DIV {class: "pg-presets"}
        (SPAN {class: "muted"} "try:")
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/templates/preset/greet')"} "greeting")
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/templates/preset/loop')"} "loop")
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/templates/preset/cond')"} "conditional")
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/templates/preset/filter')"} "filter"))
      (DIV {class: "pg-output"}
        (LABEL "output")
        (tpl-render $def_tpl $def_data)))
    (templates-files-section))
}

# theme registry: metadata + an overview builder per theme. Drives the /themes
# index and the generic /themes/:slug (+ /reference) routes, so adding a theme is
# one row here plus its overview def, no per-theme route boilerplate.
let themes = [
  [slug, title, ref, icon, blurb, overview];
  [templates, "Templates", "templates--output", "lucide:layout-template",
    "Render a page from data: an editable .mj playground, then files and the store.",
    {|| templates-overview}]
  [streaming, "Streaming & events", "streaming--events", "lucide:zap",
    "Stream chunks and push server-sent events, with a live SSE toy.",
    {|| streaming-overview}]
]

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

            (P {class: "center"} (STRONG (A {href: "/themes"} "Explore the themes ->")))

            (section-head "Why http-nu")
            (DIV {class: "grid"}
              (DIV {class: "card panel"} (H3 (icon "lucide:feather") " Tiny") (P "A single binary. Hand it a Nushell closure and you have a server."))
              (A {class: "card panel" href: "/themes/streaming"} (H3 (icon "lucide:zap") " Fast") (P "Streaming responses, SSE, and HTTP/2 over TLS out of the box."))
              (A {class: "card panel" href: "/themes/templates"} (H3 (icon "lucide:boxes") " Batteries") (P "Routing, an HTML DSL, templates, cookies, and a Datastar SDK, all embedded."))
              (DIV {class: "card panel"} (H3 (icon "lucide:database") " Stateful") (P "In-memory SQLite, a local bus, and an embedded cross.stream event store.")))
          )
          (copy-script)
        )
      )
    })

    # themes index (registry-driven)
    (route {method: GET path: "/themes"} {|req ctx|
      (HTML
        (page-head "Themes - http-nu")
        (BODY
          (nav-bar)
          (MAIN {class: "container"}
            (ARTICLE
              (H1 "Themes")
              (P {class: "muted"} "Each theme leads with something to poke, then the reference behind it.")
              (DIV {class: "grid"}
                ($themes | each {|t|
                  (A {class: "card panel" href: $"/themes/($t.slug)"}
                    (H3 (icon $t.icon) $" ($t.title)")
                    (P {class: "muted"} $t.blurb))
                }))
              (P (A {href: "/reference"} "Browse the full reference ->"))))
          (copy-script)
        )
      )
    })

    # generic theme overview + reference (registry-driven)
    (route {path-matches: "/themes/:slug"} {|req ctx|
      let t = ($themes | where slug == $ctx.slug | get 0?)
      if $t == null { ("Not Found" | metadata set { merge {'http.response': {status: 404}} }) } else {
        (theme-shell $t.slug $t.title "overview" (do $t.overview))
      }
    })
    (route {path-matches: "/themes/:slug/reference"} {|req ctx|
      let t = ($themes | where slug == $ctx.slug | get 0?)
      if $t == null { ("Not Found" | metadata set { merge {'http.response': {status: 404}} }) } else {
        let page = ($pages | where slug == $t.ref | first)
        let content = ($readme | render-page $page $anchors | inject-copy-btns)
        (theme-shell $t.slug $t.title "reference" (ARTICLE $content))
      }
    })

    # reference: the whole README, collected and anchored (single source of truth)
    (route {method: GET path: "/reference"} {|req ctx|
      let sections = ($pages | each {|p|
        (SECTION {id: $p.slug} ($readme | render-page $p $anchors | inject-copy-btns))
      })
      (HTML
        (page-head "Reference - http-nu")
        (BODY
          (nav-bar)
          (MAIN {class: "container with-sidebar"}
            (DIV {class: "docs-menu" "data-signals:nav": "false" "data-class:open": "$nav"}
              (BUTTON {class: "docs-toggle" "data-on:click": "$nav = !$nav"} "Sections")
              (NAV {class: "docs-side toc"}
                (UL ($pages | each {|p| (LI (A {href: $"/reference#($p.slug)"} $p.title))}))))
            (ARTICLE
              (P {class: "muted"} "The project README, rendered as a doc site. "
                (A {href: "/how-tos/render-readme-as-doc-site"} "Here is how ->"))
              ...$sections))
          (copy-script)
        )
      )
    })

    # how-tos: collected guides, each a Markdown file in how-tos/ rendered by .md
    (route {method: GET path: "/how-tos"} {|req ctx|
      let guides = (ls ($script_dir | path join how-tos) | where name =~ '\.md$' | sort-by name | each {|f|
        let slug = ($f.name | path basename | str replace --regex '\.md$' "")
        let title = (open --raw $f.name | decode utf-8 | lines | where {|l| ($l | str starts-with "# ")} | get 0? | default $slug | str replace "# " "")
        {slug: $slug, title: $title}
      })
      (HTML
        (page-head "How-tos - http-nu")
        (BODY
          (nav-bar)
          (MAIN {class: "container"}
            (ARTICLE
              (H1 "How-tos")
              (P {class: "muted"} "Task guides, each a Markdown file rendered the way it describes.")
              (DIV {class: "grid"}
                ($guides | each {|g|
                  (A {class: "card panel" href: $"/how-tos/($g.slug)"}
                    (H3 (icon "lucide:file-text") $" ($g.title)"))
                }))))
          (copy-script)
        )
      )
    })

    (route {path-matches: "/how-tos/:slug"} {|req ctx|
      let path = ($script_dir | path join how-tos | path join $"($ctx.slug).md")
      if ($path | path exists) {
        let content = (open --raw $path | decode utf-8 | .md | inject-copy-btns)
        (HTML
          (page-head "How-tos - http-nu")
          (BODY
            (nav-bar)
            (MAIN {class: "container"}
              (ARTICLE
                (crumbs [["How-tos" "/how-tos"]])
                $content))
            (copy-script)
          )
        )
      } else {
        ("Not Found" | metadata set { merge {'http.response': {status: 404}} })
      }
    })

    # theme interactions: streaming's live SSE buttons
    (route {method: POST path: "/themes/streaming/increment"} {|req ctx|
      let s = (from datastar-signals $req)
      let count = (($s.count? | default 0) + 1)
      {count: $count} | to datastar-patch-signals | to sse
    })
    (route {method: POST path: "/themes/streaming/time"} {|req ctx|
      (DIV {id: "stream-time"} (date now | format date "%H:%M:%S")) | to datastar-patch-elements | to sse
    })

    # theme interactions: templates' live .mj playground
    (route {method: POST path: "/themes/templates/render"} {|req ctx|
      let s = (from datastar-signals $req)
      (tpl-render ($s.tpl? | default "") ($s.data? | default "{}")) | to datastar-patch-elements | to sse
    })
    (route {path-matches: "/themes/templates/preset/:name"} {|req ctx|
      let p = (tpl-preset $ctx.name)
      [
        ({tpl: $p.tpl, data: $p.data} | to datastar-patch-signals)
        ((tpl-render $p.tpl $p.data) | to datastar-patch-elements)
      ] | to sse
    })

    # static assets (stellar.css, base.css, images)
    (route {path-matches: "/assets/:file"} {|req ctx|
      .static ($script_dir | path join "assets") $ctx.file
    })
  ]
}
