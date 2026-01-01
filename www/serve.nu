use http-nu/router *
use http-nu/datastar *
use http-nu/html *

def svg-top [] {
  SVG {viewBox: "0 0 600 70" xmlns: "http://www.w3.org/2000/svg"} (
    PATH {
      d: "M 20,65 Q 70,45 100,38"
      stroke: "currentColor"
      stroke-width: "1"
      fill: "none"
    }
  ) (
    PATH {
      id: "curve-top"
      d: "M 100,38 Q 300,15 500,38"
      fill: "none"
    }
  ) (
    TEXT {
      fill: "currentColor"
      font-family: "Georgia, serif"
      font-style: "italic"
      font-size: "28px"
    }
    (
      TEXTPATH {href: "#curve-top" startOffset: "50%" text-anchor: "middle"}
      "The surprisingly performant"
    )
  ) (
    PATH {
      d: "M 500,38 Q 530,45 580,65"
      stroke: "currentColor"
      stroke-width: "1"
      fill: "none"
    }
  )
}

def svg-bottom [] {
  SVG {viewBox: "0 0 600 70" xmlns: "http://www.w3.org/2000/svg"} (
    PATH {id: "curve-bottom" d: "M 50,15 Q 300,65 550,15" fill: "none"}
  ) (
    TEXT {
      fill: "currentColor"
      font-family: "Georgia, serif"
      font-style: "italic"
      font-size: "28px"
    }
    (
      TEXTPATH {
        href: "#curve-bottom"
        startOffset: "50%"
        text-anchor: "middle"
      } "that fits in your back pocket"
    )
  )
}

# Components

def header-bar [] {
  (
    HEADER {class: [flex items-baseline justify-between mt-10 mb-2 text-primary]}
    (DIV {class: [font-mono text-fluid-xl font-bold]} "http-nu")
    (
      NAV {class: [flex gap-4]}
      (A {href: "https://github.com/cablehead/http-nu"} "GitHub")
      (A {href: "https://discord.gg/sGgYVKnk73"} "Discord")
    )
  )
}

def badge [...args] {
  let args = if ($args | first | describe -d | get type) == 'record' and '__html' not-in ($args | first) { $args } else { [{}] | append $args }
  let attrs = $args | first
  let children = $args | skip 1
  let bg = $attrs.bg? | default "orange"
  let size = $attrs.size? | default "2xl"
  let extra = $attrs.class? | default ""
  SPAN {class: $"block w-fit mx-auto my-2 px-4 py-1 font-bold text-white shadow-offset bg-($bg) text-($size) ($extra)"} ...$children
}

def taglines [] {
  (
    DIV {class: [max-w-3xl mx-auto bg-card text-center rotate-ccw-1 pt-12 px-6 pb-2 rounded-5xl]}
    (svg-top)
    (IMG {class: [block mx-auto] style: "max-width: 90%;" src: "/ellie.png"})
    (badge {class: "rotate-ccw-3"} (A {href: "https://www.nushell.sh"} "Nushell") "-scriptable!")
    (badge {bg: "green" size: "sm" class: "rotate-cw-2 uppercase tracking-wide"} "HTTP Server")
    (DIV {class: "mt-4"} (svg-bottom))
  )
}

def icon [name: string] {
  {__html: $"<iconify-icon icon=\"($name)\" noobserver></iconify-icon>"}
}

def window-dots [] {
  [
    (SPAN {class: [w-3 h-3 rounded-full] style: "background:#ff5f56"})
    (SPAN {class: [w-3 h-3 rounded-full] style: "background:#ffbd2e"})
    (SPAN {class: [w-3 h-3 rounded-full] style: "background:#27c93f"})
  ]
}

def code-block [] {
  let code = open snippets/splash.nu
  let highlighted = $code | .highlight nu
  let copyable = $"r#'\n($code)'# | http-nu :3001 -"
  (
    DIV {class: [code rounded-lg overflow-hidden relative shadow-float basis-3/5]}
    (SCRIPT {type: "text/plain" class: "copy-content"} $copyable)
    (DIV {class: [flex items-center h-titlebar px-4 gap-2 bg-purple]} (window-dots))
    (
      BUTTON {
        class: [absolute top-2 right-3 bg-none border-none text-primary cursor-pointer text-lg p-1 transition-colors hover:text-white]
        "data-signals:copied": "false"
        "data-on:click": "$copied = true; navigator.clipboard.writeText(evt.currentTarget.closest('div').querySelector('.copy-content').textContent); setTimeout(() => $copied = false, 250)"
      }
      (SPAN {data-show: "!$copied"} (icon "mdi:content-copy"))
      (SPAN {data-show: "$copied" style: "display:none"} (icon "mdi:check"))
    )
    (
      PRE {class: [px-5 pb-5 font-mono text-code leading-relaxed overflow-x-auto text-left]}
      (SPAN {class: "comment"} "$ r#'\n")
      (CODE {class: "text-fluid-lg"} {__html: $highlighted})
      (SPAN {class: "comment"} "'# | http-nu :3001 -")
    )
  )
}

def snippet-preview [] {
  let snippet = source snippets/splash.nu
  let html = do $snippet {path: "/"}
  (
    DIV {class: [rounded-lg overflow-hidden shadow-float basis-2/5]}
    (
      DIV {class: [bg-purple flex items-center h-titlebar gap-2 px-3]}
      (window-dots)
      (DIV {class: [flex-1 bg-white rounded text-muted p-1 pl-2 ml-2]} "localhost:3001")
    )
    (DIV {class: [p-4 w-full bg-body text-primary]} $html)
  )
}

def hero [] {
  (
    DIV {class: [flex flex-col gap-4 mt-8 md:flex-row md:items-start]}
    (code-block)
    (snippet-preview)
  )
}

def install-tab [name: string label: string] {
  BUTTON {
    class: [px-4 py-2 font-mono text-sm cursor-pointer border-none transition-colors]
    "data-class:bg-dark": $"$tab === '($name)'"
    "data-class:text-white": $"$tab === '($name)'"
    "data-class:bg-none": $"$tab !== '($name)'"
    "data-class:text-primary": $"$tab !== '($name)'"
    "data-on:click": $"$tab = '($name)'"
  } $label
}

def install-content [name: string ...children] {
  DIV {
    class: [font-mono]
    "data-show": $"$tab === '($name)'"
  } ...$children
}

def wave-divider [] {
  SVG {
    class: [w-full mb-8]
    viewBox: "0 0 1200 150"
    preserveAspectRatio: "none"
    xmlns: "http://www.w3.org/2000/svg"
    style: "height: 100px; display: block;"
  } (
    PATH {
      d: "M0,50 Q300,150 600,50 T1200,50 L1200,150 L0,150 Z"
      fill: "var(--color-accent-green)"
    }
  ) (
    PATH {
      d: "M0,80 Q300,0 600,80 T1200,80 L1200,150 L0,150 Z"
      fill: "var(--color-accent-orange)"
    }
  )
}

def install-section [] {
  (
    DIV {
      class: [mt-8]
      "data-signals:tab": "'brew'"
    } (
      DIV {class: [text-2xl mb-4 font-mono flex items-center gap-2 font-bold]} "Give it a try" (
        IMG {
          src: "https://data-star.dev/cdn-cgi/image/format=auto,width=96/static/images/rocket-animated-1d781383a0d7cbb1eb575806abeec107c8a915806fb55ee19e4e33e8632c75e5.gif"
          style: "height: 1.5em;"
        }
      )
    )
    (
      DIV {class: [flex items-center h-titlebar px-4 bg-purple rounded-t-lg overflow-hidden]}
      (install-tab "brew" "Homebrew")
      (install-tab "cargo" "Cargo")
      (install-tab "eget" "eget")
      (install-tab "nix" "Nix")
    )
    (
      DIV {class: [flex items-center justify-between py-4 px-5 rounded-b-lg bg-dark]}
      (
        DIV {}
        (install-content "brew" "$ brew install cablehead/tap/http-nu")
        (install-content "cargo" "$ cargo install --locked http-nu")
        (install-content "eget" "$ eget cablehead/http-nu")
        (install-content "nix" "$ nix-shell -p http-nu")
      )
      (
        BUTTON {
          class: [bg-none border-none text-primary cursor-pointer text-lg p-1 transition-colors hover:text-white]
          "data-signals:ic": "false"
          "data-on:click": r#'
            const c = {
              brew: 'brew install cablehead/tap/http-nu',
              cargo: 'cargo install --locked http-nu',
              eget: 'eget cablehead/http-nu',
              nix: 'nix-shell -p http-nu'
            };
            navigator.clipboard.writeText(c[$tab]);
            $ic = true;
            setTimeout(() => $ic = false, 250)
          '#
        }
        (SPAN {data-show: "!$ic"} (icon "mdi:content-copy"))
        (SPAN {data-show: "$ic" style: "display:none"} (icon "mdi:check"))
      )
    )
  )
}

{|req|
  dispatch $req [
    (
      route {method: GET path: "/syntax.css"} {|req ctx|
        .response {headers: {content-type: "text/css"}}
        .highlight theme Dracula
      }
    )

    (
      route {method: GET path: "/"} {|req ctx|
        (
          HTML
          (
            HEAD
            (META {charset: "UTF-8"})
            (META {name: "viewport" content: "width=device-width, initial-scale=1.0"})
            (TITLE "http-nu")
            (META {property: "og:title" content: "http-nu"})
            (META {property: "og:description" content: "The surprisingly performant Nushell-scriptable HTTP server that fits in your back pocket"})
            (META {property: "og:image" content: "https://http-nu.cross.stream/og.png"})
            (META {property: "og:type" content: "website"})
            (META {name: "twitter:card" content: "summary_large_image"})
            (META {name: "twitter:image" content: "https://http-nu.cross.stream/og.png"})
            (LINK {rel: "stylesheet" href: "/core.css"})
            (LINK {rel: "stylesheet" href: "/syntax.css"})
            (SCRIPT {type: "module" src: $DATASTAR_CDN_URL})
            (SCRIPT {src: "https://code.iconify.design/iconify-icon/2.1.0/iconify-icon.min.js"})
          )
          (
            BODY {class: [p-2 md:p-8] data-init: "@get('/_sse')"}
            (header-bar)
            (taglines)
            (wave-divider)
            (install-section)
            (hero)
          )
        )
      }
    )

    (
      route {method: GET path: "/screenshots"} {|req ctx|
        let files = glob "assets/_/*.png" | sort -r
        (
          HTML
          (
            HEAD
            (META {charset: "UTF-8"})
            (META {name: "viewport" content: "width=device-width, initial-scale=1.0"})
            (TITLE "Screenshots")
            (
              STYLE r#'
              body { background: #1a1a2e; color: #f4d9a0; font-family: monospace; padding: 2rem; }
              h1 { margin-bottom: 1.5rem; }
              .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(400px, 1fr)); gap: 1.5rem; }
              .item { background: #282a36; border-radius: 0.5rem; overflow: hidden; }
              .item img { width: 100%; display: block; }
              .item .name { padding: 0.75rem; font-size: 0.75rem; color: #888; word-break: break-all; }
              a { color: inherit; }
              '#
            )
          )
          (
            BODY
            (H1 "Screenshots")
            (
              DIV {class: "grid"} {
                $files | each {|f|
                  let name = $f | path basename
                  let url = $"/_/($name)"
                  (
                    DIV {class: "item"}
                    (A {href: $url target: "_blank"} (IMG {src: $url}))
                    (DIV {class: "name"} $name)
                  )
                }
              }
            )
          )
        )
      }
    )

    (
      route true {|req ctx|
        .static "assets" $req.path
      }
    )
  ]
}
