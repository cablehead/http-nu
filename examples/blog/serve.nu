# Blog - http-nu edition
# Demonstrates routing, HTML generation, and layout composition.
#
# Run: http-nu :3001 examples/blog/serve.nu

use http-nu/router *
use http-nu/html *

# Sample blog posts data
let posts = [
  {id: 1, title: "Getting Started with Nushell", slug: "getting-started-nushell", date: "2026-03-20", excerpt: "Learn the basics of Nushell scripting"}
  {id: 2, title: "Building Web Servers with http-nu", slug: "building-web-servers", date: "2026-03-15", excerpt: "Create fast, Nushell-scriptable HTTP servers"}
  {id: 3, title: "Static Site Generation", slug: "static-site-generation", date: "2026-03-10", excerpt: "Generate static blogs from markdown"}
]

# Render post card for listing
def post-card [req post] {
  LI (
    ARTICLE {class: "post-card"}
      (H3 (A {href: ($req | href $"/posts/($post.slug)")} $post.title))
      (P {class: "meta"} $post.date)
      (P $post.excerpt)
  )
}

# Layout wrapper
def page-layout [req title: string content] {
  HTML
    (HEAD
      (META {charset: "utf-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE $title)
      (STYLE {__html: $"
        body { font-family: system-ui, -apple-system, sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; line-height: 1.6; }
        header { border-bottom: 2px solid #333; padding-bottom: 20px; margin-bottom: 30px; }
        nav a { margin-right: 20px; text-decoration: none; color: #0066cc; }
        nav a:hover { text-decoration: underline; }
        .post-card { border: 1px solid #ddd; padding: 15px; margin-bottom: 15px; border-radius: 4px; }
        .post-card h3 { margin: 0 0 10px 0; }
        .post-card a { color: #0066cc; text-decoration: none; }
        .post-card a:hover { text-decoration: underline; }
        .meta { color: #666; font-size: 0.9em; margin: 5px 0; }
        .post-content { margin: 40px 0; }
      "})
    )
    (BODY
      (HEADER
        (H1 "My Blog")
        (NAV
          (A {href: ($req | href "/")} "Home")
          (A {href: ($req | href "/about")} "About")
        )
      )
      $content
      (FOOTER
        (P {style: {color: "#999" "font-size": "0.9em" "margin-top": "40px" "border-top": "1px solid #ddd" "padding-top": "20px"}}
          "Built with http-nu and Nushell"
        )
      )
    )
}

# Home page - list all posts
def home [req] {
  page-layout $req "Blog" (
    MAIN
      (H2 "Latest Posts")
      (UL { $posts | each {|p| post-card $req $p } })
  )
}

# Single post page
def post-detail [req slug: string] {
  let post = ($posts | where slug == $slug | first)

  if ($post == null) {
    return ("Not Found" | metadata set --merge {'http.response': {status: 404}})
  }

  page-layout $req $post.title (
    MAIN {class: "post-content"}
      (ARTICLE
        (H2 $post.title)
        (P {class: "meta"} $post.date)
        (P "This is a placeholder for the full post content. In a real blog, this would be loaded from markdown files.")
        (P (A {href: ($req | href "/")} "<- Back to home"))
      )
  )
}

# About page
def about [req] {
  page-layout $req "About" (
    MAIN
      (H2 "About This Blog")
      (P "This is a simple blog server built with http-nu and Nushell.")
      (P "It demonstrates basic routing, HTML generation, and layout composition.")
      (P (A {href: ($req | href "/")} "<- Back to home"))
  )
}

# Main request handler
{|req|
  dispatch $req [
    # Home page
    (route {path: "/"} {|req ctx|
      home $req
    })

    # About page
    (route {path: "/about"} {|req ctx|
      about $req
    })

    # Single post
    (route {path-matches: "/posts/:slug"} {|req ctx|
      post-detail $req $ctx.slug
    })

    # 404 fallback
    (route true {|req ctx|
      page-layout $req "Not Found" (
        MAIN
          (H2 "Page Not Found")
          (P "The page you're looking for doesn't exist.")
          (P (A {href: ($req | href "/")} "<- Back to home"))
      ) | metadata set --merge {'http.response': {status: 404}}
    })
  ]
}
