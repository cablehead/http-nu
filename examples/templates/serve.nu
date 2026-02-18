{|req|
  match $req.path {
    "/file" => { {name: "World"} | .mj "page.html" }
    "/topic" => { {name: "World"} | .mj --topic "page.topic" }
    _ => {
      {} | .mj --inline '<h1>Templates</h1>
<ul>
  <li><a href="/file">File mode</a> - extends and include from disk</li>
  <li><a href="/topic">Topic mode</a> - extends and include from store</li>
</ul>'
    }
  }
}
