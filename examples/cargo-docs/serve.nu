# Serve cargo doc output
#
# Usage:
#   cargo doc --workspace --no-deps
#   http-nu :3001 examples/cargo-docs/serve.nu
#
# Set DOC_ROOT to customize the path (defaults to ./target/doc)

use http-nu/html *
use http-nu/router *

let doc_root = ($env.DOC_ROOT? | default "target/doc")

# Build an index page listing all documented crates
def index-page [] {
  let crates = ls $doc_root
    | where type == dir
    | each { get name | path basename }
    | where {|c| ($doc_root | path join $c "index.html" | path exists) }
    | sort

  HTML (HEAD
    (META {charset: "utf-8"})
    (TITLE "Docs")
    (STYLE {__html: "
      body { font-family: system-ui, sans-serif; max-width: 600px; margin: 2rem auto; padding: 0 1rem; }
      a { color: #2563eb; text-decoration: none; }
      a:hover { text-decoration: underline; }
      li { margin: 0.4rem 0; font-family: monospace; }
    "})
  ) (BODY
    (H1 "Crates")
    (UL { $crates | each {|c| LI (A {href: $"/($c)/"} $c) } })
  )
}

{|req|
  dispatch $req [
    (route {path: "/"} {|req ctx|
      index-page
    })

    (route true {|req ctx|
      .static $doc_root $req.path
    })
  ]
}
