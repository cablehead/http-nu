#!/usr/bin/env nu

use http-nu/router *
use http-nu/datastar *

{|req|
  [
    # SSE stream endpoint
    (route {path: "/stream"} {|req ctx|
      .response {headers: {"content-type": "text/event-stream"}}

      tail -F ./quotes.json
      | lines
      | each {|line|
        let q = $line | from json
        {quote: $q.quote, who: ($q.who? | default "")} | to sse-patch-signals
      }
    })

    # Serve static files (default)
    (route true {|req ctx|
      .static "www" $req.path --fallback "index.html"
    })
  ]
  | dispatch $req
}
