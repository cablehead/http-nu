#!/usr/bin/env nu

use http-nu/router *
use http-nu/datastar *
use http-nu/html *

{|req|
  [
    # SSE stream endpoint
    (route {path: "/stream"} {|req ctx|
      tail -F ./quotes.json
      | lines
      | each {|line|
        let q = $line | from json
        h-div {id: "quote"} {||
          h-p {class: "quote"} $q.quote
          | h-p {class: "who"} $"- ($q.who? | default 'Anonymous')"
        }
        | to dstar-patch-element
      }
      | to sse
    })

    # Serve static files (default)
    (route true {|req ctx|
      .static "www" $req.path --fallback "index.html"
    })
  ]
  | dispatch $req
}
