# Seed store topics from disk files when --store is enabled
if $HTTP_NU.store != null {
  open examples/templates/topics/page.html | .append page.html
  open examples/templates/topics/base.html | .append base.html
  open examples/templates/topics/nav.html | .append nav.html
}

{|req|
  match $req.path {
    "/file" => { {name: "World"} | .mj "examples/templates/page.html" }
    "/topic" => { {name: "World"} | .mj --topic "page.html" }
    _ => {
      {} | .mj --inline '<h1>Templates</h1>
<p>This page is rendered with <code>.mj --inline</code>.</p>
<ul>
  <li><a href="./file">File mode</a> - extends and include from disk</li>
  <li><a href="./topic">Topic mode</a> - extends and include from store</li>
</ul>'
    }
  }
}
