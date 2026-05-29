# CF Workspace browser example.
#
# Workspace lookup: the default DurableObject unless the URL uses an
# explicit `/u/<user>/` prefix (then that user's DO is used and the
# prefix is stripped before this handler sees the request).
#
# Routes (default DO):
#   GET  /                    -> file browser HTML
#   GET  /file?path=X         -> read file content
#   POST /file?path=X         -> write file (body is content)
#   POST /mkdir?path=X        -> create directory (recursive)
#   POST /rm?path=X           -> remove (recursive, force)
#   POST /cp?path=A&dst=B     -> copy A to B
#   POST /mv?path=A&dst=B     -> rename A to B
#   GET  /exists?path=X       -> "true" / "false"
#   GET  /static/<path>       -> serve from /assets/<path> with
#                                Content-Type from extension
#
# Per-user mode: prefix any URL with /u/<name>/, e.g.
#   GET /u/alice/file?path=/note.txt
#
# Nu's `ls`, `open`, `save`, `path exists`, `mkdir`, `rm`, `cp`, `mv`,
# `.static` are shadowed to the per-request Workspace snapshot.

{|req|
  # Capture body first; subsequent pipelines might steal $in.
  let body = $in
  let route = ($req.path? | default "/")
  let method = ($req.method? | default "GET")
  let query = ($req.query? | default {})
  let qpath = ($query | get -i path | default "/note.txt")

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
<p>Default DO. For per-user routing, use <code>/u/&lt;name&gt;/</code> URLs.</p>
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
