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

# each page's h3 sub-sections, precomputed at load (parsing deep in the DSL tree
# at request time overflows the stack). Map: page slug -> [{slug, title}].
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
    # social + search metadata (shared by every page via this head)
    (META {name: "description" content: "A tiny, Nushell-scriptable HTTP server: hand it a closure and you have a server. Streaming, SSE, templates, and an embedded event store, all in one binary."})
    (META {property: "og:title" content: $title})
    (META {property: "og:type" content: "website"})
    (META {property: "og:site_name" content: "http-nu"})
    (META {property: "og:description" content: "A tiny, Nushell-scriptable HTTP server with streaming, SSE, templates, and an embedded event store."})
    (META {name: "twitter:card" content: "summary"})
    (LINK {rel: "icon" type: "image/png" href: "/assets/ellie.png"})
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
    (SCRIPT {src: "/assets/vt-tuner.js"})
  )
}

def theme-toggle [] {
  (BUTTON {
    id: "theme-toggle"
    title: "Toggle theme"
    onclick: "var d=document.documentElement.classList.toggle('dark');localStorage.setItem('theme',d?'dark':'light')"
  } (icon "lucide:sun-moon"))
}

# launcher for the page-transition tuner (wired up by vt-tuner.js)
def vt-toggle [] {
  (BUTTON {
    id: "vt-tuner-btn"
    class: "vt-toggle"
    type: "button"
    title: "Page transition tuner"
  } (icon "ph:toolbox"))
}

# Shared nav: brand + links + theme toggle, on every page.
def nav-bar [] {
  (NAV {class: "nav"}
    (DIV {class: "brand"} (A {href: "/"} "http-nu"))
    (DIV {class: "links"}
      (A {href: "/themes"} "Themes")
      (A {href: "/tutorials"} "Tutorials")
      (A {href: "/how-tos"} "How-tos")
      (A {href: "/reference"} "Reference")
      (A {href: "https://github.com/cablehead/http-nu"} "GitHub")
      (vt-toggle)
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

# site footer: shared across every page (added beside copy-script in each body)
def site-footer [] {
  (FOOTER {class: "site-footer"}
    (DIV {class: "container"}
      (P {class: "muted"}
        (A {href: "https://github.com/cablehead/http-nu"} "GitHub") " \u{b7} "
        (A {href: "https://discord.com/invite/YNbScHBHrh"} "Discord") " \u{b7} "
        (A {href: "/tutorials"} "Tutorials") " \u{b7} "
        (A {href: "/how-tos"} "How-tos") " \u{b7} "
        (A {href: "/reference"} "Reference") " \u{b7} "
        (A {href: "/design"} "Design") " \u{b7} "
        (A {href: "https://www.nushell.sh"} "Nushell") "-scriptable, " (A {href: "https://cross.stream"} "cross.stream")
        "-powered, " (A {href: "https://data-star.dev"} "Datastar") "-ready")))
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
    (P "http-nu is a single binary. Grab it with your preferred package manager:")
    (terminal
      ($methods | each {|m|
        BUTTON {class: "terminal-tab" "data-class:is-active": $"$tab === '($m.0)'" "data-on:click": $"$tab = '($m.0)'"} $m.1
      })
      ($methods | each {|m|
        DIV {"data-show": $"$tab === '($m.0)'"} (SPAN {class: "prompt"} "$ ") $m.2
      })))
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
    (code-toy $run_src "bash")
    (P (A {href: "/themes/streaming/reference"} "Full reference: streaming responses, to sse, and streaming input ->")))
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

# reference sidebar: every section; the current one expands to its sub-sections
def ref-nav [current: string] {
  let items = ($pages | each {|p|
    let active = ($p.slug == $current)
    let sub = (if $active {
      let secs = ($page_secs | get $p.slug)
      if ($secs | is-empty) { [] } else {
        [(UL ($secs | each {|s| LI (A {href: $"/reference/($p.slug)#($s.slug)"} $s.title)}))]
      }
    } else { [] })
    (LI (A {href: $"/reference/($p.slug)" class: (if $active { "active" } else { "" })} $p.title) ...$sub)
  })
  (NAV {class: "docs-side toc"} (UL $items))
}

# the first H1 of a Markdown doc, or a fallback
def md-title [fallback: string]: string -> string {
  lines | where {|l| ($l | str starts-with "# ")} | get 0? | default $fallback | str replace "# " ""
}

# Breadcrumb trail: Section / Page, every crumb a link (the last to itself).
# One universal scheme; callers pass [[section href] [page self-href]].
def crumbs [trail: list] {
  let parts = ($trail | enumerate | each {|it|
    let link = (A {href: ($it.item | get 1)} ($it.item | get 0))
    if $it.index > 0 { [" / " $link] } else { [$link] }
  } | flatten)
  (P {class: "muted crumbs"} ...$parts)
}

# page shell shared by every routed page: head, nav, the given main, then footer.
def page [title: string, main] {
  (HTML {lang: "en"}
    (page-head $title)
    (BODY
      (nav-bar)
      $main
      (site-footer)
      (copy-script)))
}

def theme-shell [theme: string, title: string, current: string, body] {
  let trail = [["Themes" "/themes"] [$title $"/themes/($theme)"]]
  (page $"($title) - http-nu"
    (MAIN {class: "container"}
      (crumbs $trail)
      (H1 $title)
      (theme-facets $theme $current)
      $body))
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

def gb-ensure [] { try { stor create -t guestbook -c {msg: str} } catch {} }
def guestbook-list [] {
  gb-ensure
  let rows = (stor open | query db "select msg from guestbook order by rowid desc limit 8")
  (DIV {id: "guestbook" class: "guestbook"}
    (if ($rows | is-empty) {
      (P {class: "muted"} "no messages yet")
    } else {
      (UL ($rows | each {|r| (LI $r.msg)}))
    }))
}

def storage-overview [] {
  let stor_src = r#'
# stor is an in-memory SQLite table: no server, no file
stor create -t guestbook -c {msg: str}
"hello" | wrap msg | stor insert -t guestbook
stor open | query db "select * from guestbook"'#
  (ARTICLE
    (P "Keep state without a database server: an in-memory SQLite via " (CODE "stor")
      ", a local pub/sub " (CODE "bus") ", and an embedded cross.stream event store. "
      "Here is " (CODE "stor") ", live:")
    (DIV {class: "playground" "data-signals": "{msg: ''}"}
      (P {class: "muted"} "A shared in-memory table. Sign it; rows last until the server restarts.")
      (DIV {class: "pg-live"}
        (INPUT {class: "pg-input" "data-bind:msg": true placeholder: "leave a message" spellcheck: "false"})
        (BUTTON {class: "chip" "data-on:click": "@post('/themes/storage/sign')"} "Sign"))
      (guestbook-list))
    (H2 "How it works")
    (toy "Insert and query" "stor is an in-memory SQLite table you can insert into and query with SQL, no setup." $stor_src "nu")
    (P (A {href: "/themes/storage/reference"} "Full reference: stor, the bus, and the event store ->")))
}

# styled 404 page, with the 404 status set on the response
def not-found [] {
  (page "Not found - http-nu"
    (MAIN {class: "container"}
      (ARTICLE
        (H1 "Not found")
        (P {class: "muted"} "That page does not exist.")
        (P (A {href: "/"} "Back home ->")))))
  | metadata set { merge {'http.response': {status: 404}} }
}

def tut-gb-ensure [] { try { stor create -t signatures -c {name: str, msg: str} } catch {} }
def tut-gb-list [] {
  tut-gb-ensure
  let rows = (stor open | query db "select name, msg from signatures order by rowid desc limit 8")
  (DIV {id: "tut-guestbook" class: "guestbook"}
    (if ($rows | is-empty) {
      (P {class: "muted"} "no signatures yet")
    } else {
      (UL ($rows | each {|r| (LI (STRONG $r.name) $" -- ($r.msg)")}))
    }))
}

# the hello-world tutorial: a simulated terminal. Enter boots the server (its
# banner), then each curl click appends a response line.
# reusable terminal window chrome (the splash .terminal component): a titlebar
# with the mac dots + a label, and a body.
def terminal [title, body, --action: any = null] {
  (DIV {class: "terminal"}
    (DIV {class: "terminal-bar"}
      (SPAN {class: "terminal-dots"} (SPAN) (SPAN) (SPAN))
      (SPAN {class: "terminal-title"} $title)
      (if $action != null { (SPAN {class: "terminal-action"} $action) } else { "" }))
    (DIV {class: "terminal-body"} $body))
}

# one server + client terminal pair, scripted like a screencast (no real
# server): "Start Server" reveals the captured boot banner and lights the key;
# "Run" reveals the request's log line and the curl response. s_sig / c_sig are
# the Datastar signal names that drive each reveal.
def demo-pair [
  server_cmd: string
  client_cmd: string
  banner: string
  reqlog: string
  s_sig: string
  c_sig: string
  response
] {
  let s_show = ("$" + $s_sig)
  let c_show = ("$" + $c_sig)
  [
    (terminal "server" --action (BUTTON {class: "chip-go" "data-class:is-lit": $s_show "data-on:click": ($s_show + " = true")} "Start Server")
      (DIV
        (DIV {class: "term-cmd"} (SPAN {class: "prompt"} "$ ") (CODE $server_cmd))
        (DIV {class: "term-out"}
          (PRE {"data-show": $s_show} $banner)
          (DIV {"data-show": $c_show} $reqlog))))
    (DIV {class: "client-pane" "data-show": $s_show}
      (terminal "client" --action (BUTTON {class: "chip" "data-on:click": ($c_show + " = true")} "Run")
        (DIV
          (DIV {class: "term-cmd"} (SPAN {class: "prompt"} "$ ") (CODE $client_cmd))
          (DIV {class: "term-out" "data-show": $c_show} $response))))
  ]
}

# the bare interactive widget: a scripted hello-world (string), then the same
# again returning a record (json, curled with -v). Droppable into any page;
# framing prose (intro, next link) is supplied by the caller.
def hello-world-demo [] {
  let banner = (open ($script_dir | path join demo-banner.txt))
  (DIV {"data-signals": "{started: false, curled: false, started2: false, curled2: false}"}
    (DIV {class: "terminal-row"}
      (demo-pair "http-nu :3001 -c '{|req| \"Hello, world!\"}'" "curl localhost:3001"
        $banner "23:37:42.108  127.0.0.1 GET / 200 0ms 0ms 13b" "started" "curled"
        "Hello, world!"))
    (P "The string is returned as " (CODE "text/html") " by default. Return a record "
      "and it becomes " (CODE "application/json") " automatically.")
    (DIV {class: "terminal-row"}
      (demo-pair "http-nu :3003 -c '{|req| {msg: \"Hello, world!\"} }'" "curl -v localhost:3003"
        ($banner | str replace --all "3001" "3003") "23:38:55.102  127.0.0.1 GET / 200 0ms 0ms 23b" "started2" "curled2"
        (DIV (DIV "< HTTP/1.1 200 OK") (DIV "< content-type: application/json") (DIV "{\"msg\":\"Hello, world!\"}")))))
}

def tutorial-hello-world [] {
  (DIV
    (P "http-nu takes a Nushell closure and serves it over HTTP. The closure "
      "receives the request as its argument, and whatever it returns becomes the "
      "response. Boot the smallest possible server, then curl it:")
    (hello-world-demo)
    (P (A {href: "/tutorials/build-a-live-guestbook"} "Next: build a live guestbook ->")))
}

# the getting-started tutorial: lead with the finished guestbook (live), then the
# step-by-step build rendered from its Markdown.
def tutorial-getting-started [] {
  let md = (open --raw ($script_dir | path join tutorials build-a-live-guestbook.md) | decode utf-8 | .md | inject-copy-btns)
  (DIV
    (P {class: "muted"} "From hello-world to real-time updates. Here is the finished guestbook, live: sign it, then build it step by step below.")
    (DIV {class: "playground" "data-signals": "{name: '', message: ''}"}
      (DIV {class: "pg-live"}
        (INPUT {class: "pg-input" "data-bind:name": true placeholder: "your name" spellcheck: "false"})
        (INPUT {class: "pg-input" "data-bind:message": true placeholder: "a message" spellcheck: "false"})
        (BUTTON {class: "chip" "data-on:click": "@post('/tutorials/guestbook/sign')"} "Sign"))
      (P {class: "muted"} (SMALL "This demo keeps signatures in memory with stor; the tutorial below builds the persistent, cross-tab version with the event store and SSE."))
      (tut-gb-list))
    (H2 "Build it step by step")
    $md)
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
  [storage, "State & storage", "state--storage", "lucide:database",
    "In-memory SQLite, a local bus, and an embedded event store. Sign the live guestbook.",
    {|| storage-overview}]
]

# tutorials registry: step-by-step builds, each with an interactive lead widget.
let tutorials = [
  [slug, title, blurb, builder];
  ["hello-world", "Hello world",
    "The smallest http-nu server: a closure that returns a string. Boot it and curl it, right here.",
    {|| tutorial-hello-world}]
  ["build-a-live-guestbook", "Build a live guestbook",
    "From hello-world to real-time updates: the HTML DSL, routing, the store, and Datastar SSE.",
    {|| tutorial-getting-started}]
]

# --- design system viewer --------------------------------------------
# /design catalogs the site's vocabulary. Each component page shows a live
# example, then breaks the component into its parts; each part lists the
# design tokens behind it (a swatch for colors).

# one token row: a role label, an optional color swatch, the token name.
def tok [label: string, token: string, --color] {
  let bg = (if ($token | str starts-with "--") { $"var\(($token)\)" } else { $token })
  let sw = (if $color {
    (SPAN {class: "tok-sw" style: $"background: ($bg)"})
  } else {
    (SPAN {class: "tok-sw tok-sw-none"})
  })
  (DIV {class: "tok"}
    (SPAN {class: "tok-label"} $label)
    $sw
    (CODE {class: "tok-name"} $token))
}

# one part of a component: name, gloss, an optional focused demo, its tokens.
def design-part [name: string, note: string, demo, tokens: list] {
  (SECTION {class: "dz-part"}
    (H3 $name)
    (P {class: "muted"} $note)
    (if $demo != null { (DIV {class: "dz-demo"} $demo) } else { "" })
    (DIV {class: "dz-tokens"} ...$tokens))
}

def design-window [] {
  let example = (terminal "server"
    --action (BUTTON {class: "chip"} "Run")
    (DIV
      (DIV {class: "term-cmd"} (SPAN {class: "prompt"} "$ ") (CODE "http-nu :3001 -c '{|req| \"hi\"}'"))
      (DIV {class: "term-out"} "hi")))
  (DIV
    (DIV {class: "dz-example"} $example)
    (design-part "Frame" "The window itself: rounded, shadowed, monospace." null [
      (tok "radius" "--border-radius-2")
      (tok "shadow" "--shadow-2")
      (tok "font" "--font-mono")
      (tok "size" "--font-size--1")
    ])
    (design-part "Title bar" "Purple chrome strip: dots, title slot, action slot." null [
      (tok "background" "--named-purple-0" --color)
      (tok "text" "--named-purple-0-on" --color)
      (tok "padding" "0.2rem / --size--1")
      (tok "gap" "--size--1")
    ])
    (design-part "Traffic lights" "Three mac-style dots, decorative." (SPAN {class: "terminal-dots"} (SPAN) (SPAN) (SPAN)) [
      (tok "size" "0.7rem")
      (tok "red" "--named-red-0" --color)
      (tok "amber" "#ffbd2e" --color)
      (tok "green" "#27c93f" --color)
    ])
    (design-part "Title slot" "Flex slot holding a label (server) or a tab strip (install methods)." null [
      (tok "gap" "--size--1")
    ])
    (design-part "Action" "Optional control, pushed to the far edge." null [
      (tok "align" "margin-left: auto")
    ])
    (design-part "Body" "The dark content area: code, output, anything." null [
      (tok "background" "rgba(0,0,0,0.25)" --color)
      (tok "text" "--surface-on" --color)
      (tok "padding" "--size--2")
      (tok "line-height" "--code-line-height")
    ]))
}

let design_catalog = [
  [slug, title, blurb, builder];
  ["window", "Window",
    "A chrome panel for code and output: a title bar over a dark body. The install tabs and the run demos are all this one component.",
    {|| design-window}]
]

def design-nav [current: string] {
  (NAV {class: "docs-side toc"}
    (UL ($design_catalog | each {|c|
      (LI (A {href: $"/design/($c.slug)" class: (if $c.slug == $current { "active" } else { "" })} $c.title))
    })))
}

def design-page [slug: string] {
  let entry = ($design_catalog | where slug == $slug | first)
  (page $"($entry.title) - Design - http-nu"
    (MAIN {class: "container with-sidebar"}
      (DIV {class: "docs-menu" "data-signals:nav": "false" "data-class:open": "$nav"}
        (BUTTON {class: "docs-toggle" "data-on:click": "$nav = !$nav"} "Components")
        (design-nav $slug))
      (ARTICLE
        (crumbs [["Design" "/design"] [$entry.title $"/design/($slug)"]])
        (H1 $entry.title)
        (P {class: "muted"} $entry.blurb)
        (do $entry.builder))))
}

{|req|
  dispatch $req [

    # landing page
    (route {method: GET path: "/"} {|req ctx|
      (HTML {lang: "en"}
        (page-head "http-nu")
        (BODY
          (splash-hero)
          (MAIN {class: "container"}
            (give-it-a-try)

            (P "Then hand it a Nushell closure, and you have a server:")
            (DIV {class: "demo-wide"} (hello-world-demo))

            (section-head "Why http-nu")
            (DIV {class: "grid"}
              ($themes | each {|t|
                (A {class: "card panel" href: $"/themes/($t.slug)"}
                  (H3 (icon $t.icon) $" ($t.title)")
                  (P {class: "muted"} $t.blurb))
              }))
          )
          (site-footer)
          (copy-script)
        )
      )
    })

    # themes index (registry-driven)
    (route {method: GET path: "/themes"} {|req ctx|
      (page "Themes - http-nu"
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
            (P (A {href: "/reference"} "Browse the full reference ->")))))
    })

    # generic theme overview + reference (registry-driven)
    (route {path-matches: "/themes/:slug"} {|req ctx|
      let t = ($themes | where slug == $ctx.slug | get 0?)
      if $t == null { (not-found) } else {
        (theme-shell $t.slug $t.title "overview" (do $t.overview))
      }
    })
    (route {path-matches: "/themes/:slug/reference"} {|req ctx|
      let t = ($themes | where slug == $ctx.slug | get 0?)
      if $t == null { (not-found) } else {
        let page = ($pages | where slug == $t.ref | first)
        let content = ($readme | render-page $page $anchors --base "/reference" | inject-copy-btns)
        (theme-shell $t.slug $t.title "reference" (ARTICLE $content))
      }
    })

    # reference index: section cards (the README, section by section)
    (route {method: GET path: "/reference"} {|req ctx|
      (page "Reference - http-nu"
        (MAIN {class: "container"}
          (ARTICLE
            (H1 "Reference")
            (P {class: "muted"} "The project README, section by section. "
              (A {href: "/how-tos/render-readme-as-doc-site"} "Rendered as a doc site ->"))
            (DIV {class: "grid"}
              ($pages | each {|p|
                let secs = ($page_secs | get $p.slug)
                (A {class: "card panel" href: $"/reference/($p.slug)"}
                  (H3 $p.title)
                  (if ($secs | is-empty) { "" } else { (SMALL ($secs | get title | str join " \u{b7} ")) }))
              })))))
    })

    # reference section: one README section with sidebar + pager
    (route {path-matches: "/reference/:slug"} {|req ctx|
      let idx = ($pages | enumerate | where item.slug == $ctx.slug | get index.0? | default null)
      if $idx == null {
        (not-found)
      } else {
        let page = ($pages | get $idx)
        let prev = (if $idx > 0 { $pages | get ($idx - 1) } else { null })
        let next = ($pages | get -o ($idx + 1))
        let content = ($readme | render-page $page $anchors --base "/reference" | inject-copy-btns)
        (page $"($page.title) - http-nu"
          (MAIN {class: "container with-sidebar"}
            (DIV {class: "docs-menu" "data-signals:nav": "false" "data-class:open": "$nav"}
              (BUTTON {class: "docs-toggle" "data-on:click": "$nav = !$nav"} "Sections")
              (ref-nav $page.slug))
            (ARTICLE
              (crumbs [["Reference" "/reference"] [$page.title $"/reference/($page.slug)"]])
              $content
              (NAV {class: "pager"}
                (if ($idx > 0) {
                  (A {class: "pager-link panel pager-prev" href: $"/reference/($prev.slug)"}
                    (SPAN {class: "pager-dir"} (icon "lucide:arrow-left") "Previous")
                    (SPAN {class: "pager-page"} $prev.title))
                } else { "" })
                (if ($next != null) {
                  (A {class: "pager-link panel pager-next" href: $"/reference/($next.slug)"}
                    (SPAN {class: "pager-dir"} "Next" (icon "lucide:arrow-right"))
                    (SPAN {class: "pager-page"} $next.title))
                } else { "" })))))
      }
    })

    # how-tos: collected guides, each a Markdown file in how-tos/ rendered by .md
    (route {method: GET path: "/how-tos"} {|req ctx|
      let guides = (ls ($script_dir | path join how-tos) | where name =~ '\.md$' | sort-by name | each {|f|
        let slug = ($f.name | path basename | str replace --regex '\.md$' "")
        let title = (open --raw $f.name | decode utf-8 | md-title $slug)
        {slug: $slug, title: $title}
      })
      (page "How-tos - http-nu"
        (MAIN {class: "container"}
          (ARTICLE
            (H1 "How-tos")
            (P {class: "muted"} "Task guides, each a Markdown file rendered the way it describes.")
            (DIV {class: "grid"}
              ($guides | each {|g|
                (A {class: "card panel" href: $"/how-tos/($g.slug)"}
                  (H3 (icon "lucide:file-text") $" ($g.title)"))
              })))))
    })

    (route {path-matches: "/how-tos/:slug"} {|req ctx|
      let path = ($script_dir | path join how-tos | path join $"($ctx.slug).md")
      if ($path | path exists) {
        let raw = (open --raw $path | decode utf-8)
        let title = ($raw | md-title $ctx.slug)
        (page $"($title) - http-nu"
          (MAIN {class: "container"}
            (ARTICLE
              (crumbs [["How-tos" "/how-tos"] [$title $"/how-tos/($ctx.slug)"]])
              ($raw | .md | inject-copy-btns))))
      } else {
        (not-found)
      }
    })

    # tutorials: step-by-step builds with an interactive lead widget
    (route {method: GET path: "/tutorials"} {|req ctx|
      (page "Tutorials - http-nu"
        (MAIN {class: "container"}
          (ARTICLE
            (H1 "Tutorials")
            (P {class: "muted"} "Step-by-step builds you can poke as you go.")
            (DIV {class: "grid"}
              ($tutorials | each {|t|
                (A {class: "card panel" href: $"/tutorials/($t.slug)"}
                  (H3 (icon "lucide:graduation-cap") $" ($t.title)")
                  (P {class: "muted"} $t.blurb))
              })))))
    })
    (route {path-matches: "/tutorials/:slug"} {|req ctx|
      let t = ($tutorials | where slug == $ctx.slug | get 0?)
      if $t == null { (not-found) } else {
        (page $"($t.title) - http-nu"
          (MAIN {class: "container"}
            (ARTICLE
              (crumbs [["Tutorials" "/tutorials"] [$t.title $"/tutorials/($t.slug)"]])
              (H1 $t.title)
              (do $t.builder))))
      }
    })
    (route {method: POST path: "/tutorials/guestbook/sign"} {|req ctx|
      let s = (from datastar-signals $req)
      let name = ($s.name? | default "" | str trim)
      let message = ($s.message? | default "" | str trim)
      tut-gb-ensure
      if ($name != "" and $message != "") { {name: $name, msg: $message} | stor insert -t signatures }
      [ ({name: "", message: ""} | to datastar-patch-signals) ((tut-gb-list) | to datastar-patch-elements) ] | to sse
    })
    # design system viewer: /design redirects to the first component
    (route {method: GET path: "/design"} {|req ctx|
      let first = ($design_catalog | first | get slug)
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $"/design/($first)"}}} }
    })
    (route {path-matches: "/design/:slug"} {|req ctx|
      let entry = ($design_catalog | where slug == $ctx.slug | get 0?)
      if $entry == null { (not-found) } else { (design-page $ctx.slug) }
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
    # theme interactions: storage's live stor guestbook
    (route {method: POST path: "/themes/storage/sign"} {|req ctx|
      let s = (from datastar-signals $req)
      let msg = ($s.msg? | default "" | str trim)
      gb-ensure
      if ($msg != "") { $msg | wrap msg | stor insert -t guestbook }
      [ ({msg: ""} | to datastar-patch-signals) ((guestbook-list) | to datastar-patch-elements) ] | to sse
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
