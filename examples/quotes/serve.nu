use http-nu/router *
use http-nu/datastar *
use http-nu/html *

def quote-html []: record -> string {
  let q = $in
  _div {id: "quote"} {
    _p {style: "font-style: italic;"} $"\"($q.quote)\""
    | +p {style: "color: #666; text-align: right;"} $"— ($q.who? | default 'Anonymous')"
  }
}

{|req|
  dispatch $req [
    (
      route {method: GET path: "/" has-header: {accept: "text/event-stream"}} {|req ctx|
        tail -F ./quotes.json
        | lines
        | each {|line|
          $line | from json | quote-html | to dstar-patch-element
        }
        | to sse
      }
    )

    (
      route {method: GET path: "/"} {|req ctx|
        _html {
          _head {
            _meta {charset: "utf-8"}
            | +title "Live Quotes"
            | +script {type: "module" src: $DATASTAR_CDN_URL}
          }
          | +body {"data-init": "@get('/')" style: "font-family: Georgia, serif; padding: 2rem;"} (
            {quote: "Waiting for quotes..."} | quote-html
          )
        }
      }
    )
  ]
}
