use http-nu/router *
use http-nu/datastar *
use http-nu/html *

def quote-html []: record -> string {
  let q = $in
  _div {
    id: "quote"
    style: {
      background-color: "#e8e6e3"
      color: "#4a4a4a"
      height: 100dvh
      padding: "5vh 10vw"
      font-size: 6vmax
      display: flex
      flex-direction: column
      justify-content: center
      overflow: hidden
    }
  } {
    _p {
      style: {
        font-family: "Georgia, serif"
        font-style: italic
        text-align: center
      }
    } $"\"($q.quote)\""
    | +p {
      style: {
        font-family: "'American Typewriter', Courier, monospace"
        font-size: 4vmax
        text-align: right
        margin-top: 10vh
      }
    } $"— ($q.who? | default 'Anonymous')"
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
            | +style "* { box-sizing: border-box; margin: 0; }"
            | +script {type: "module" src: $DATASTAR_CDN_URL}
          }
          | +body {data-init: "@get('/')"} (
            {quote: "Waiting for quotes..."} | quote-html
          )
        }
      }
    )
  ]
}
