# stor.nu - In-memory SQLite example for http-nu
#
# Demonstrates using Nushell's `stor` commands to maintain an in-memory
# SQLite database that persists across requests. The database is
# concurrent-safe but does not survive server restarts.
#
# Run with: http-nu :3001 examples/stor.nu
#
# Setup:  curl -X POST localhost:3001/setup
# Insert: curl -X POST localhost:3001/visit/hello
# Query:  curl localhost:3001/visits

{|req|
  let path = $req.path

  if $path == "/" {
    "<html><body>
      <h1>stor example</h1>
      <ul>
        <li><a href='./setup'>POST /setup</a> -- create visits table</li>
        <li><a href='./visits'>GET /visits</a> -- list all visits</li>
        <li><a href='./count'>GET /count</a> -- visit counts by path</li>
        <li><a href='./visit/hello'>POST /visit/:name</a> -- record a visit</li>
      </ul>
    </body></html>"
  } else if $path == "/setup" {
    stor create -t visits -c {path: str, ts: str, method: str} | ignore
    "table created"
  } else if ($path | str starts-with "/visit/") {
    let name = $path | str replace "/visit/" ""
    stor insert -t visits -d {
      path: $name
      ts: (date now | format date "%Y-%m-%d %H:%M:%S")
      method: $req.method
    } | ignore
    $"recorded visit to ($name)"
  } else if $path == "/visits" {
    stor open | query db "select * from visits"
  } else if $path == "/count" {
    let rows = stor open | query db "select path, count(*) as n from visits group by path"
    $rows
  } else {
    "404 - not found" | metadata set --merge {'http.response': {status: 404}}
  }
}
