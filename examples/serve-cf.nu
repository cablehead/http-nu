# http-nu examples hub -- CF target
#
# Same shape as examples/serve.nu (the desktop hub) but only mounts
# demos that work on Cloudflare Workers today.
#
# Excluded:
#   - stor      : `stor *` family unported to wasm
#   - templates : `.append` cross-stream not ported
#   - quotes    : `.last --follow` cross-stream not ported
#   - 2048      : `.append` (cross-stream backend in upstream serve.nu).
#                 Local-bus variant `2048-animation` also blocked
#                 (`.bus pub/sub` not yet ported to wasm).
#   - tao       : top-level `let slides = open data.json` -- needs
#                 assets seeded BEFORE the hub parses. Use the
#                 standalone task `mise run ex:cf:tao` + `DEMO=tao
#                 mise run cf:seed:demo` instead.
#   - cargo-docs: same -- standalone via `mise run ex:cf:cargo-docs`
#                 + `mise run cf:seed:cargo-docs`.
#
# Track xs / bus port in src/cf/nu/nu_command/xs/PLAN.md.
#
# Run on CF: mise run cf:dev:hub
# Bundler:   scripts/bundle-cf-handler.nu (inlines `source X.nu`).

use http-nu/router *
use http-nu/html *

let basic = source basic.nu
let counter = source datastar-counter/serve.nu
let sdk = source datastar-sdk/serve.nu
let mermaid = source mermaid-editor/serve.nu
let blog = source blog/serve.nu
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
    (example-link "./basic/" "basic" "minimal routes, JSON (AVOID /basic/time: sleep is no-op on CF, generate-loop dies)")
    (example-link "./datastar-counter/" "datastar-counter" "reactive counter")
    (example-link "./datastar-sdk/" "datastar-sdk" "SDK feature demo")
    (example-link "./datastar-sdk-test/" "datastar-sdk-test" "SDK test runner (POST /test)")
    (example-link "./mermaid-editor/" "mermaid-editor" "live diagram editor (seed: DEMO=mermaid-editor mise run cf:seed:demo)")
    (example-link "./blog/" "blog" "routing, layouts, HTML composition"))
    (P {class: "skipped"} "Standalone-only (need pre-seeded assets): tao, cargo-docs. Run via mise run ex:cf:tao or ex:cf:cargo-docs.")
    (P {class: "skipped"} "Blocked on cross-stream / bus / stor port (see src/cf/nu/nu_command/xs/PLAN.md): 2048, 2048-animation, stor, templates, quotes."))
  })

  (mount "/basic" $basic)
  (mount "/datastar-counter" $counter)
  (mount "/datastar-sdk" $sdk)
  (mount "/datastar-sdk-test" $sdk_test)
  (mount "/mermaid-editor" $mermaid)
  (mount "/blog" $blog)
]

{|req|
  dispatch $req $routes
}
