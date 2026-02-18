# The Tao of Datastar - http-nu edition
# A port of https://github.com/1363V4/tao
#
# Run: http-nu --datastar --dev -w :3001 examples/tao/serve.nu

use http-nu/router *
use http-nu/datastar *
use http-nu/http *

let db = (open examples/tao/data.json)
let page = (.mj compile "examples/tao/page.html")

def make-ctx [state: record, reps: int] {
  let light = ($reps / 100)

  let nav_js = if $reps == 100 {
    "null"
  } else {
    match [($state | get -i previous) ($state | get -i next)] {
      [null, null] => "null"
      [null, _] => $"evt.key === 'ArrowRight' ? window.location = '($state.next)' : null"
      [_, null] => $"evt.key === 'ArrowLeft' ? window.location = '($state.previous)' : null"
      _ => $"evt.key === 'ArrowLeft' ? window.location = '($state.previous)' : evt.key === 'ArrowRight' ? window.location = '($state.next)' : null"
    }
  }

  {
    title: $state.title
    content: $state.content
    previous: ($state | get -i previous)
    next: ($state | get -i next)
    light: $light
    vt_duration: (2 - 2 * $light)
    nav_js: $nav_js
    datastar_js_path: $DATASTAR_JS_PATH
  }
}

{|req|
  dispatch $req [
    (route {|req|
      if ($req.path | str starts-with "/static/") {
        {subpath: ($req.path | str replace "/static" "")}
      }
    } {|req ctx|
      .static "examples/tao/static" $ctx.subpath
    })

    (route {path: "/"} {|req ctx|
      let cookies = $req | cookie parse
      let reps = ($cookies | get -i reps | default "0" | into int)
      make-ctx ($db | get state) $reps | .mj render $page | cookie set reps $"($reps)"
    })

    (route {path: "/state"} {|req ctx|
      let cookies = $req | cookie parse
      let reps = ($cookies | get -i reps | default "0" | into int)
      let new_reps = if $reps > 0 { [($reps + 10) 100] | math min } else { $reps }
      make-ctx ($db | get state) $new_reps | .mj render $page | cookie set reps $"($new_reps)"
    })

    (route {path-matches: "/:key"} {|req ctx|
      if ($ctx.key in $db) {
        let cookies = $req | cookie parse
        let reps = ($cookies | get -i reps | default "0" | into int)
        make-ctx ($db | get $ctx.key) $reps | .mj render $page | cookie set reps $"($reps)"
      } else {
        "404 not found" | metadata set --merge {'http.response': {status: 404}}
      }
    })
  ]
}
