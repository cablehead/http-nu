# http-nu examples hub -- CF target
#
# Same shape as examples/serve.nu (the desktop hub) but only mounts
# demos that work on Cloudflare Workers today. Skipped on CF:
#   - stor      : `stor *` family unported to wasm
#   - templates : top-level `.append page.html` (cross-stream, no wasm port)
#   - quotes    : `.last quotes --follow` (cross-stream)
# Track all four ports in src/cf/nu/nu_command/xs/PLAN.md.
#
# Run on CF: mise run cf:dev:hub
# Bundler:   scripts/bundle-cf-handler.nu (inlines `source X.nu`).
# Seed assets: mise run cf:seed:demo  with DEMO=<demo-name>.

use http-nu/router *
use http-nu/html *

let basic = source basic.nu
let counter = source datastar-counter/serve.nu
let sdk = source datastar-sdk/serve.nu
let mermaid = source mermaid-editor/serve.nu
let blog = source blog/serve.nu
let game_2048 = source 2048/serve.nu
let tao = source tao/serve.nu
let cargo_docs = source cargo-docs/serve.nu
let generate_test = source generate-test/serve.nu
let sdk_test = source datastar-sdk-test/serve.nu

def example-link [href: string label: string desc: string] {
  LI (A {href: $href} $label) $" --($desc)"
}

let routes = [
  (route {method: GET path: "/"} {|req ctx|
    HTML (HEAD
    (META {charset: "UTF-8"})
    (META {name: "viewport" content: "width=device-width, initial-scale=1"})
    (TITLE "http-nu examples (CF)")
    (STYLE "
body { font-family: system-ui, sans-serif; max-width: 600px; margin: 2rem auto; padding: 0 1rem; }
a { color: #2563eb; }
li { margin: 0.5rem 0; }
.skipped { color: #9ca3af; font-size: 0.85em; }
")) (BODY
    (H1 "http-nu examples (CF)")
    (P "Demos verified on local wrangler dev + Cloudflare Workers.")
    (UL
    (example-link "./basic/" "basic" "minimal routes, JSON. AVOID /basic/time -- sleep is a no-op on CF, the generate loop will spin until the worker dies")
    (example-link "./datastar-counter/" "datastar-counter" "reactive counter")
    (example-link "./datastar-sdk/" "datastar-sdk" "SDK feature demo")
    (example-link "./datastar-sdk-test/" "datastar-sdk-test" "SDK test runner (POST /test)")
    (example-link "./mermaid-editor/" "mermaid-editor" "live diagram editor")
    (example-link "./generate-test/" "generate-test" "stock `generate` exercise")
    (example-link "./blog/" "blog" "routing, layouts, HTML composition")
    (example-link "./tao/" "tao" "Tao of Datastar (seed: DEMO=tao mise run cf:seed:demo)")
    (example-link "./cargo-docs/" "cargo-docs" "browse cargo docs (seed: mise run cf:seed:cargo-docs)")
    (example-link "./2048/" "2048" "solo 2048 (home only; gameplay needs .bus sub)"))
    (P {class: "skipped"} "Skipped on CF (need xs / stor port -- see src/cf/nu/nu_command/xs/PLAN.md): stor, templates, quotes."))
  })

  (mount "/basic" $basic)
  (mount "/datastar-counter" $counter)
  (mount "/datastar-sdk" $sdk)
  (mount "/datastar-sdk-test" $sdk_test)
  (mount "/mermaid-editor" $mermaid)
  (mount "/generate-test" $generate_test)
  (mount "/blog" $blog)
  (mount "/tao" $tao)
  (mount "/cargo-docs" $cargo_docs)
  (mount "/2048" $game_2048)
]

{|req|
  dispatch $req $routes
}
