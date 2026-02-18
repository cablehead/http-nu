# The Tao of Datastar - http-nu edition
# A port of https://github.com/1363V4/tao
#
# Run: http-nu --datastar --dev -w :3001 examples/tao/serve.nu

use http-nu/router *
use http-nu/datastar *
use http-nu/html *
use http-nu/http *

let db = (open examples/tao/data.json)

def make-page [state: record, reps: int] {
  let light = ($reps / 100)
  let vt_duration = (2 - 2 * $light)

  # keyboard navigation js
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

  let custom_css = $":root {
    --light: ($light);
}
::view-transition-group\(*\) {
    animation-duration: ($vt_duration)s;
}"

  let prev_link = if ($state | get -i previous) != null {
    A {href: $state.previous} {__html: "&#x2B05;&#xFE0F;"}
  } else {
    P ""
  }

  let next_link = if ($state | get -i next) != null {
    A {href: $state.next} {__html: "&#x27A1;&#xFE0F;"}
  } else {
    ""
  }

  (HTML
    {lang: "en"}
    (HEAD
      (META {charset: "UTF-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE "The Tao")
      (LINK {rel: "icon" href: "/static/img/favicon.ico"})
      (LINK {rel: "stylesheet" href: "/static/css/site.css"})
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
      (STYLE $custom_css)
    )
    (BODY
      {class: "gc" "data-on:keydown.window.throttle_1000ms": $nav_js}
      (DIV {id: "container"}
        (H1 $state.title)
        (P $state.content)
        (DIV {id: "arrow-container"}
          $prev_link
          $next_link
        )
      )
    )
  )
  | cookie set reps $"($reps)"
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
      make-page ($db | get state) $reps
    })

    (route {path: "/state"} {|req ctx|
      let cookies = $req | cookie parse
      let reps = ($cookies | get -i reps | default "0" | into int)
      let new_reps = if $reps > 0 { [($reps + 10) 100] | math min } else { $reps }
      make-page ($db | get state) $new_reps
    })

    (route {path-matches: "/:key"} {|req ctx|
      if ($ctx.key in $db) {
        let cookies = $req | cookie parse
        let reps = ($cookies | get -i reps | default "0" | into int)
        make-page ($db | get $ctx.key) $reps
      } else {
        "404 not found" | metadata set --merge {'http.response': {status: 404}}
      }
    })
  ]
}
