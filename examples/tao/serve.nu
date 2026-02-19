# The Tao of Datastar - http-nu edition
# A port of https://github.com/1363V4/tao
#
# Run: http-nu --datastar --dev -w :3001 examples/tao/serve.nu

# breathe.
# state in the right place.
# data flows in, HTML flows out. that is all.

const script_dir = path self | path dirname

use http-nu/router *
use http-nu/datastar *
use http-nu/http *
use http-nu/html *

# in Nushell, `open` understands JSON, YAML, TOML, CSV, SQLite...
# no libraries, no boilerplate. just open.
let slides = open ($script_dir | path join data.json)

# a template is compiled once, rendered many times.
let page = .mj compile ($script_dir | path join page.html)

# keyboard navigation adapts to where we are.
# pattern matching on the shape of the data -
# four cases, four paths, no ambiguity.
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

# view as a function of state.
# data comes in through the pipe, gets shaped into context,
# rendered through the template, and sent on its way.
def render-slide [req: record name: string] {
  let slide = $slides | get $name
  let cookies = $req | cookie parse
  let reps = $cookies | get -i reps | default "0" | into int

  # each full reading, the world gets a little brighter.
  let reps = $reps | if ($name == "state") { [($in + 10) 100] | math min } else { $in }
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

# a request arrives. we listen, we respond.
# each route is a pattern. the first match wins.
{|req|
  dispatch $req [
    (
      route {|req|
        if ($req.path | str starts-with "/static/") {
          {subpath: ($req.path | str replace "/static" "")}
        }
      } {|req ctx|
        .static ($script_dir | path join static) $ctx.subpath
      }
    )

    # the journey begins here.
    (
      route {path: "/"} {|req ctx|
        render-slide $req "state"
      }
    )

    # or you may arrive at any point along the way.
    (
      route {path-matches: "/:key"} {|req ctx|
        if ($ctx.key in $slides) {
          render-slide $req $ctx.key
        } else {
          P [
            "You have strayed from the path. Breathe. Find your way back, "
            (A {href: "/"} "to the tao")
          ] | metadata set { merge {'http.response': {status: 404}} }
        }
      }
    )
  ]
}
