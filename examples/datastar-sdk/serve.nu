use http-nu/router *
use http-nu/datastar *
use http-nu/html *

{|req|
  dispatch $req [
    # Index page
    (route {path: "/"} {|req ctx|
      _html {
        _head {
          _meta {charset: "UTF-8"}
          | append (_title "Datastar SDK Demo")
          | append (_script {type: "module" src: "https://cdn.jsdelivr.net/gh/starfederation/datastar@1.0.0-RC.7/bundles/datastar.js"})
        }
        | append (_body {"data-signals": "{count: 0}"} {
          _h1 "Datastar SDK Demo"
          | append (_div {style: "display: flex; gap: 2em;"} {
            _div {
              _h3 "to dstar-patch-signal"
              | append (_p ("Count: " | append (_span {"data-text": "$count"} "0") | str join))
              | append (_button {"data-on:click": "@post('/increment')"} "Increment")
            }
            | append (_div {
              _h3 "to dstar-execute-script"
              | append (_button {"data-on:click": "@post('/hello')"} "Say Hello")
            })
            | append (_div {
              _h3 "to dstar-patch-element"
              | append (_div {id: "time"} "--:--:--.---")
              | append (_button {"data-on:click": "@post('/time')"} "Get Time")
            })
          })
        })
      }
    })

    # Increment counter signal
    (route {method: "POST" path: "/increment"} {|req ctx|
      let signals = from datastar-request $req
      let count = ($signals.count? | default 0) + 1
      {count: $count} | to dstar-patch-signal | to sse
    })

    # Execute script on client
    (route {method: "POST" path: "/hello"} {|req ctx|
      "alert('Hello from the server!')" | to dstar-execute-script | to sse
    })

    # Update time div
    (route {method: "POST" path: "/time"} {|req ctx|
      let time = date now | format date "%H:%M:%S%.3f"
      _div {id: "time"} $time | to dstar-patch-element | to sse
    })
  ]
}
