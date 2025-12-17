use http-nu/router *
use http-nu/datastar *
use http-nu/html *

{|req|
  dispatch $req [
    # Index page
    (
      route {method: GET path: "/"} {|req ctx|
        _html {
          _head {
            _meta {charset: "UTF-8"}
            | +title "Datastar SDK Demo"
            | +script {type: "module" src: $DATASTAR_CDN_URL}
          }
          | +body {"data-signals": "{count: 0}"} {
            _h1 "Datastar SDK Demo"
            | +div {style: "display: flex; gap: 2em;"} {
              _div {
                _h3 "to dstar-patch-signal"
                | +p ("Count: " | +span {"data-text": "$count"} "0" | str join)
                | +button {"data-on:click": "@post('/increment')"} "Increment"
              }
              | +div {
                _h3 "to dstar-execute-script"
                | +button {"data-on:click": "@post('/hello')"} "Say Hello"
              }
              | +div {
                _h3 "to dstar-patch-element"
                | +div {id: "time"} "--:--:--.---"
                | +button {"data-on:click": "@post('/time')"} "Get Time"
              }
            }
          }
        }
      }
    )

    # Increment counter signal
    (
      route {method: POST path: "/increment"} {|req ctx|
        let signals = from datastar-request $req
        let count = ($signals.count? | default 0) + 1
        {count: $count} | to dstar-patch-signal | to sse
      }
    )

    # Execute script on client
    (
      route {method: POST path: "/hello"} {|req ctx|
        "alert('Hello from the server!')" | to dstar-execute-script | to sse
      }
    )

    # Update time div
    (
      route {method: POST path: "/time"} {|req ctx|
        let time = date now | format date "%H:%M:%S%.3f"
        _div {id: "time"} $time | to dstar-patch-element | to sse
      }
    )
  ]
}
