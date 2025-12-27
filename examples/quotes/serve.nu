use http-nu/router *
use http-nu/datastar *
use http-nu/html *

def quote-html []: record -> record {
  let q = $in
  DIV {
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
  } (
    P {
      style: {
        font-family: "Georgia, serif"
        font-style: italic
        text-align: center
      }
    } $"\"($q.quote)\""
    P {
      style: {
        font-family: "'American Typewriter', Courier, monospace"
        font-size: 4vmax
        text-align: right
        margin-top: 10vh
      }
    } $"— ($q.who? | default 'Anonymous')"
  )
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
        (
          HTML
          (
            HEAD
            (META {charset: "utf-8"})
            (TITLE "Live Quotes")
            (STYLE "* { box-sizing: border-box; margin: 0; }")
            (SCRIPT {type: "module" src: $DATASTAR_CDN_URL})
          )
          (
            BODY {data-init: "@get('/')"}
            ({quote: "Waiting for quotes..."} | quote-html)
          )
        )
      }
    )
  ]
}
