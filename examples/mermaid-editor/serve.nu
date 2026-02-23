use http-nu/router *
use http-nu/datastar *
use http-nu/html *

const static_dir = path self | path dirname | path join assets

let default_source = "graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Action 1]
    B -->|No| D[Action 2]"

def html-escape []: string -> string {
  str replace --all '&' '&amp;'
  | str replace --all '<' '&lt;'
  | str replace --all '>' '&gt;'
}

def mermaid-el []: string -> record {
  let escaped = ($in | html-escape)
  {__html: $"<mermaid-diagram id=\"preview\">($escaped)</mermaid-diagram>"}
}

{|req|
  dispatch $req [
    (
      route {method: POST path: "/"} {|req ctx|
        let signals = (from datastar-signals $req)
        $signals.source | mermaid-el | to datastar-patch-elements | to sse
      }
    )

    (
      route {method: GET path: "/"} {|req ctx|
        (
          HTML
          (HEAD
            (META {charset: "UTF-8"})
            (META {name: "viewport" content: "width=device-width, initial-scale=1"})
            (TITLE "dia2")
            (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
            (SCRIPT {type: "module" src: "/mermaid-diagram.js"})
            (STYLE "
* { box-sizing: border-box; margin: 0; padding: 0; }
body { height: 100dvh; display: flex; font-family: system-ui, sans-serif; }
.pane { flex: 1; padding: 1rem; display: flex; flex-direction: column; min-width: 0; }
textarea {
  flex: 1; resize: none;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 14px; line-height: 1.5;
  padding: 1rem; border: 1px solid #ddd; border-radius: 4px; outline: none;
}
textarea:focus { border-color: #4a9eff; }
mermaid-diagram {
  flex: 1; border: 1px solid #ddd; border-radius: 4px;
  padding: 1rem; overflow: auto;
  display: flex; align-items: center; justify-content: center;
}
")
          )
          (BODY
            (DIV {class: "pane"}
              (TEXTAREA {
                "data-bind:source": true
                "data-on:input__debounce.500ms": "@post('/')"
              } $default_source)
            )
            (DIV {class: "pane"}
              ($default_source | mermaid-el)
            )
          )
        )
      }
    )

    (route true {|req ctx|
      .static $static_dir $req.path
    })
  ]
}
