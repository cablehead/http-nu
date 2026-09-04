# Render a markdown file with `.md`, then upgrade ```mermaid fences to
# live diagrams in the browser.
#
# Run: http-nu :3001 examples/markdown-mermaid/serve.nu
#
# `.md` has no per-language hook, so a mermaid fence comes out as a plain
# <pre><code class="language-mermaid"> block. assets/mermaid-fences.js
# swaps each of those for the <mermaid-diagram> web component from the
# mermaid-editor example, which is served from that example's assets dir.

use http-nu/router *
use http-nu/html *

const here = path self | path dirname
const content = $here | path join content page.md
const static_dir = $here | path join assets
const component_dir = $here | path join .. mermaid-editor assets
const mermaid_src = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs"

# Raw CSS. Plain strings passed to STYLE are HTML-escaped, which would
# mangle quotes and the `>` combinator, so wrap in {__html: ...}.
const css = {__html: "
body { font-family: system-ui, sans-serif; max-width: 720px; margin: 2rem auto; padding: 0 1rem; line-height: 1.5; }
pre { padding: 1rem; border: 1px solid #ddd; border-radius: 4px; overflow: auto; }
code { font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace; font-size: 14px; }
mermaid-diagram { display: block; min-height: 16rem; padding: 1rem; border: 1px solid #ddd; border-radius: 4px; }

/* Avoid a flash of the diagram source: once the inline script below has
   flagged the document as JS-capable, hide mermaid fences before first
   paint. mermaid-fences.js replaces them with <mermaid-diagram> once the
   module loads. Without JS the flag is never set and the source stays
   visible. */
.js pre:has(> code.language-mermaid) { display: none; }
"}

{|req|
  dispatch $req [
    (route {method: GET path: "/"} {|req ctx|
      let body = open --raw $content | decode utf-8 | .md
      (HTML
      (HEAD
      (META {charset: "UTF-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE "markdown + mermaid")
      # Classic inline script: runs synchronously during parsing, before
      # the body renders, so the CSS above applies from the first paint.
      (SCRIPT {__html: "document.documentElement.classList.add('js')"})
      (LINK {rel: "modulepreload" href: $mermaid_src})
      (SCRIPT {type: "module" src: "./mermaid-fences.js"})
      (STYLE {__html: (.highlight theme GitHub)})
      (STYLE $css))
      (BODY (MAIN $body)))
    })

    (route {method: GET path: "/mermaid-diagram.js"} {|req ctx|
      .static $component_dir $req.path
    })

    (route true {|req ctx|
      .static $static_dir $req.path
    })
  ]
}
