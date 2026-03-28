# stor.nu - In-memory SQLite example for http-nu
#
# Demonstrates using Nushell's `stor` commands to maintain an in-memory
# SQLite database that persists across requests. Every page load records
# itself, so the example is self-demonstrating.
#
# Run with: http-nu :3001 examples/stor.nu

use http-nu/html *

stor create -t visits -c {path: str, ts: str, method: str} | ignore

{|req|
  # Record every request
  {
    path: $req.path
    ts: (date now | format date "%Y-%m-%d %H:%M:%S")
    method: $req.method
  } | stor insert -t visits | ignore

  let counts = stor open | query db "select path, count(*) as n from visits group by path order by n desc"
  let recent = stor open | query db "select * from visits order by ts desc limit 10"

  HTML (HEAD
    (META {charset: "utf-8"})
    (TITLE "stor example")
    (STYLE {__html: "
      body { font-family: system-ui, sans-serif; max-width: 600px; margin: 2rem auto; padding: 0 1rem; }
      table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
      th, td { border: 1px solid #ddd; padding: 6px 10px; text-align: left; }
      th { background: #f5f5f5; }
      .muted { color: #666; font-size: 0.9em; }
      .try-links { display: flex; flex-wrap: wrap; gap: 10px; margin: 1rem 0; }
      .btn {
        display: inline-block; padding: 8px 16px; border-radius: 6px;
        text-decoration: none; font-size: 0.95em; border: 1px solid #ddd;
        background: #f8f8f8; color: #2563eb;
      }
      a.btn:hover { background: #eef; }
      a.btn:visited { color: #2563eb; }
      .btn.current, .btn.current:visited, .btn.current:hover { background: #2563eb; color: #fff; border-color: #2563eb; font-weight: bold; }
    "})
  ) (BODY
    (H1 "stor example")
    (P "Nushell has built-in "
      (A {href: "https://www.nushell.sh/commands/categories/database.html"} "stor")
      " commands for in-memory SQLite. This page uses them to log every request it receives."
    )
    (P "Click around to see the tables update:")
    (DIV {class: "try-links"} {
      let rotations = ["rotate(-2deg)" "rotate(1.5deg)" "rotate(-1deg)" "rotate(2.5deg)"]
      [
        ["/" "./"]
        ["/tacos" "./tacos"]
        ["/warp-drive" "./warp-drive"]
        ["/banana-phone" "./banana-phone"]
      ] | enumerate | each {|it|
        let link = $it.item
        let rot = $rotations | get $it.index
        A {class: (if $req.path == $link.0 { "btn current" } else { "btn" }) href: $link.1 style: {transform: $rot}} $link.0
      }
    })

    (H2 "Hits by path")
    (TABLE
      (THEAD (TR (TH "path") (TH "count")))
      (TBODY { $counts | each {|r| TR (TD $r.path) (TD ($r.n | into string)) } })
    )

    (H2 "Recent visits")
    (TABLE
      (THEAD (TR (TH "path") (TH "method") (TH "time")))
      (TBODY { $recent | each {|r| TR (TD $r.path) (TD $r.method) (TD $r.ts) } })
    )
  )
}
