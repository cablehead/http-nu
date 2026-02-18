# The Tao of Datastar - http-nu edition
# A port of https://github.com/1363V4/tao
#
# Run: http-nu --datastar --dev -w :3001 examples/tao/serve.nu

# Breath

use http-nu/router *
use http-nu/datastar *
use http-nu/http *
use http-nu/html *

# Nushell is a better `jq` than `jq`
let slides = open examples/tao/data.json

# We pre-compile our template on server start up
let page = .mj compile "examples/tao/page.html"

# Generate keyboard navigation, depending on which slide we are on
def nav-js [slide: record reps: int] {
  if $reps == 100 {
    "null"
  } else {
    match [($slide.previous?) ($slide.next?)] {
      [null null] => "null"
      [null _] => $"evt.key === 'ArrowRight' ? window.location = '($slide.next)' : null"
      [_ null] => $"evt.key === 'ArrowLeft' ? window.location = '($slide.previous)' : null"
      _ => $"evt.key === 'ArrowLeft' ? window.location = '($slide.previous)' : evt.key === 'ArrowRight' ? window.location = '($slide.next)' : null"
    }
  }
}

def render-slide [req: record name: string] {
  let slide = $slides | get $name
  let cookies = $req | cookie parse
  let reps = $cookies | get -i reps | default "0" | into int

  # increment reps each time we loop back to the first slide.
  let reps = $reps | if ($name == "state") { [($in + 10) 100] | math min } else { $in }
  # each time, everything is a little clearer, a little quicker.
  let light = $reps / 100

  {
    title: $slide.title
    content: $slide.content
    previous: ($slide.previous?)
    next: ($slide.next?)
    light: $light
    vt_duration: (2 - 2 * $light)
    nav_js: (nav-js $slide $reps)
    datastar_js_path: $DATASTAR_JS_PATH
  }
  # send your HTML into the world. send ....
  | .mj render $page | cookie set reps $"($reps)"
}

{|req|
  dispatch $req [
    # static assets
    (
      route {|req|
        if ($req.path | str starts-with "/static/") {
          {subpath: ($req.path | str replace "/static" "")}
        }
      } {|req ctx|
        .static "examples/tao/static" $ctx.subpath
      }
    )

    # index :: show the first slide ("state")
    (
      route {path: "/"} {|req ctx|
        render-slide $req "state"
      }
    )

    # direct link to a slide page
    (
      route {path-matches: "/:key"} {|req ctx|
        if ($ctx.key in $slides) {
          render-slide $req $ctx.key
        } else {
          P [
            "You have strayed from the path. Breath. Find your way back, "
            (A {href: "/"} "to the tao")
          ] | metadata set { merge {'http.response': {status: 404}} }
        }
      }
    )
  ]
}
