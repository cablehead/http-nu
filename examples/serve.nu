# http-nu examples hub
#
# Run: http-nu --datastar :3001 examples/serve.nu
# With store: http-nu --datastar --store ./store :3001 examples/serve.nu

use http-nu/router *
use http-nu/html *

let basic = source basic.nu
let counter = source datastar-counter/serve.nu
let sdk = source datastar-sdk/serve.nu
let mermaid = source mermaid-editor/serve.nu
let templates = source templates/serve.nu
let quotes = source quotes/serve.nu

let has_store = $HTTP_NU.store != null

def mount [prefix: string handler: closure] {
  route {|req|
    if ($req.path == $prefix) or ($req.path | str starts-with $"($prefix)/") {
      {prefix: $prefix}
    }
  } {|req ctx|
    let body = $in
    if $req.path == $ctx.prefix {
      # Redirect to trailing slash for correct relative URL resolution
      "" | metadata set { merge {'http.response': {status: 302 headers: {location: $"($ctx.prefix)/"}}} }
    } else {
      let path = $req.path | str replace $ctx.prefix ""
      $body | do $handler ($req | upsert path $path)
    }
  }
}

def example-link [href: string label: string desc: string --disabled] {
  if $disabled {
    LI (SPAN {style: {color: "#9ca3af"}} $label) $" — ($desc) " (SPAN {style: {color: "#9ca3af" font-size: "0.85em"}} "(requires --store)")
  } else {
    LI (A {href: $href} $label) $" — ($desc)"
  }
}

let routes = [
  (
    route {method: GET path: "/"} {|req ctx|
      HTML (
        HEAD
        (META {charset: "UTF-8"})
        (META {name: "viewport" content: "width=device-width, initial-scale=1"})
        (TITLE "http-nu examples")
        (STYLE "
body { font-family: system-ui, sans-serif; max-width: 600px; margin: 2rem auto; padding: 0 1rem; }
a { color: #2563eb; }
li { margin: 0.5rem 0; }
")
      ) (
        BODY
        (H1 "http-nu examples")
        (UL
          (example-link "./basic/" "basic" "minimal routes, JSON, streaming")
          (example-link "./datastar-counter/" "datastar-counter" "reactive counter")
          (example-link "./datastar-sdk/" "datastar-sdk" "SDK feature demo")
          (example-link "./mermaid-editor/" "mermaid-editor" "live diagram editor")
          (example-link "./templates/" "templates" ".mj template modes")
          (example-link "./quotes/" "quotes" "live quotes board" --disabled=(not $has_store))
        )
      )
    }
  )

  (mount "/basic" $basic)
  (mount "/datastar-counter" $counter)
  (mount "/datastar-sdk" $sdk)
  (mount "/mermaid-editor" $mermaid)
  (mount "/templates" $templates)
  ...(if $has_store {
    [(mount "/quotes" $quotes)]
  } else {
    []
  })
]

{|req|
  dispatch $req $routes
}
