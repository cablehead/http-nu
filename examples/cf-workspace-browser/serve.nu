# CF Workspace browser example.
#
# Each user (URL first path segment) gets their own isolated workspace,
# backed by DO SQLite + R2 (1.5MB spill threshold) via the http-nu Rust
# port of @cloudflare/shell.
#
# Routes (path is the full URL including the /{user}/ prefix):
#
#   GET  /{user}/                    -> file browser HTML
#   GET  /{user}/file?path=X         -> read file content
#   POST /{user}/file?path=X         -> write file (body is content)
#   POST /{user}/mkdir?path=X        -> create directory (recursive)
#   POST /{user}/rm?path=X           -> remove (recursive, force)
#   POST /{user}/cp?path=A&dst=B     -> copy A to B
#   POST /{user}/mv?path=A&dst=B     -> rename A to B
#   GET  /{user}/exists?path=X       -> "true" / "false"
#   GET  /{user}/static/<path>       -> serve from /assets/<path> with
#                                       Content-Type from extension
#
# Nu's `ls`, `open`, `save`, `path exists`, `mkdir`, `rm`, `cp`, `mv`,
# `.static` are shadowed to the per-request Workspace snapshot.

{|req|
  # Capture body first; subsequent pipelines might steal $in.
  let body = $in
  let path = ($req.path? | default "/")
  let method = ($req.method? | default "GET")
  let query = ($req.query? | default {})
  let qpath = ($query | get -i path | default "/note.txt")

  let parts = ($path | split row "/" | skip 1)
  let route = if (($parts | length) <= 1) {
    "/"
  } else {
    "/" + ($parts | skip 1 | str join "/")
  }

  # NOTE: no `return` -- it strips PipelineData metadata (e.g. the
  # Content-Type header .static attaches). Use if/else expressions
  # whose value is the closure's result.
  if $route == "/" {
    let entries = (ls /)
    let rows = ($entries | each {|e|
      let link = if $e.type == "file" {
        $'<a href="file?path=/($e.name)">($e.name)</a>'
      } else {
        $e.name
      }
      $"<tr><td>($link)</td><td>($e.type)</td><td>($e.size)</td></tr>"
    } | str join "")
    # __html: ... triggers text/html Content-Type via response inference.
    {__html: $"<!DOCTYPE html><html><head><title>Workspace</title>
<style>body{font-family:system-ui;max-width:800px;margin:2em auto;padding:0 1em}
table{width:100%;border-collapse:collapse}td,th{padding:.4em;border-bottom:1px solid #eee;text-align:left}</style></head>
<body><h1>Workspace</h1>
<p>user: <code>($parts | get 0)</code></p>
<table><thead><tr><th>name</th><th>type</th><th>size</th></tr></thead>
<tbody>($rows)</tbody></table>
</body></html>"}
  } else if ($route | str starts-with "/file") {
    if $method == "POST" {
      let text = ($body | default "" | into string)
      $text | save -f $qpath
      $"saved ($qpath): ($text | str length) bytes"
    } else {
      open $qpath
    }
  } else if $route == "/mkdir" {
    mkdir $qpath
    $"mkdir ($qpath) ok"
  } else if $route == "/rm" {
    rm -rf $qpath
    $"rm ($qpath) ok"
  } else if $route == "/cp" {
    let dst = ($query | get -i dst | default "/copy.txt")
    cp $qpath $dst
    $"cp ($qpath) -> ($dst) ok"
  } else if $route == "/mv" {
    let dst = ($query | get -i dst | default "/renamed.txt")
    mv $qpath $dst
    $"mv ($qpath) -> ($dst) ok"
  } else if $route == "/exists" {
    let yes = ($qpath | path exists)
    $"($qpath): ($yes)"
  } else if ($route | str starts-with "/static") {
    let sub = ($route | str substring 7..)
    let sub = if ($sub | is-empty) { "/" } else { $sub }
    .static "/assets" $sub
  } else {
    $"404 -- unknown route ($route)"
  }
}
