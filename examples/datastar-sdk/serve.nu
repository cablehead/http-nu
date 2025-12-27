use http-nu/router *
use http-nu/datastar *
use http-nu/html *

{|req|
  dispatch $req [
    # Index page
    (
      route {method: GET path: "/"} {|req ctx|
        (HTML
          (HEAD
            (META {charset: "UTF-8"})
            (TITLE "Datastar SDK Demo")
            (SCRIPT {type: "module" src: $DATASTAR_CDN_URL}))
          (BODY {"data-signals": "{count: 0}"}
            (H1 "Datastar SDK Demo")
            (DIV {style: {display: flex gap: 2em}}
              (DIV
                (H3 "to dstar-patch-signal")
                (P "Count: " (SPAN {"data-text": "$count"} "0"))
                (BUTTON {"data-on:click": "@post('/increment')"} "Increment"))
              (DIV
                (H3 "to dstar-execute-script")
                (BUTTON {"data-on:click": "@post('/hello')"} "Say Hello"))
              (DIV
                (H3 "to dstar-patch-element")
                (DIV {id: "time"} "--:--:--.---")
                (BUTTON {"data-on:click": "@post('/time')"} "Get Time")))))
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
        DIV {id: "time"} $time | to dstar-patch-element | to sse
      }
    )
  ]
}
