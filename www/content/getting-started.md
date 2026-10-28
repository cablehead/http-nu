# Getting Started: Build a Live Guestbook

Build a guestbook app with http-nu, step by step. By the end you will have
a server with composable HTML, persistent storage, and real-time updates
powered by Datastar and server-sent events.

## Step 1: Hello World

http-nu takes a Nushell closure and serves it over HTTP. The closure receives
the request as its argument. Whatever it returns becomes the response.

```bash
http-nu :3001 -c '{|req| "Hello, world!"}'
```

Test it:

```bash
$ curl localhost:3001
Hello, world!
```

The string is returned as `text/html` by default. Return a record and it
becomes `application/json` automatically.

## Step 2: The HTML DSL

One-liners are fun, but let's build a real page. Create a file called
`serve.nu`:

```nu
use http-nu/html *

{|req|
  HTML [
    (HEAD [
      (META {charset: "UTF-8"})
      (TITLE "Guestbook")
    ])
    (BODY [
      (H1 "Guestbook")
      (P "Welcome! Sign the guestbook below.")
      (UL [
        (LI [(STRONG "Alice") " -- Hello, world!"])
        (LI [(STRONG "Bob") " -- Great site!"])
      ])
    ])
  ]
}
```

Run it:

```bash
http-nu :3001 serve.nu
```

Tags are uppercase Nushell commands: `H1`, `P`, `UL`, `DIV`. The first
argument can be an attribute record: `{class: "intro"}`. Everything after
that is children -- strings, other tags, or lists. Plain strings are
auto-escaped for XSS protection.

`HTML` prepends `<!DOCTYPE html>`. `class` accepts a list:
`{class: [bold italic]}`. Boolean attributes work too: `{disabled: true}`.

## Step 3: Routing

Every request currently hits the same handler, so even a request for a
stylesheet would get the guestbook page back. Let's add routing with the
built-in router module, use it to add an About page, and serve files from
a `static` folder next to `serve.nu`.

Start with a stylesheet. Save this as `static/style.css`. It styles raw
HTML elements only, so the page needs no classes:

```css
:root { color-scheme: light dark; }
body {
  max-width: 40rem;
  margin: 2rem auto;
  padding: 0 1rem;
  font-family: system-ui, sans-serif;
  line-height: 1.5;
}
input, textarea, button { font: inherit; }
```

Then update `serve.nu`:

```nu
use http-nu/html *
use http-nu/router *

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      HTML [
        (HEAD [
          (META {charset: "UTF-8"})
          (TITLE "Guestbook")
          (LINK {rel: "stylesheet" href: "/style.css"})
        ])
        (BODY [
          (H1 "Guestbook")
          (P "No messages yet.")
          (P (A {href: "/about"} "About this guestbook"))
        ])
      ]
    })

    (route {method: "GET" path: "/about"} {|req ctx|
      HTML [
        (HEAD [
          (META {charset: "UTF-8"})
          (TITLE "About")
          (LINK {rel: "stylesheet" href: "/style.css"})
        ])
        (BODY [
          (H1 "About")
          (P "A guestbook built with http-nu.")
          (P (A {href: "/"} "Back"))
        ])
      ]
    })

    (route true {|req ctx| .static "static" $req.path })
  ]
}
```

`dispatch` tests routes in order -- first match wins. Each `route` takes a
test (a record for matching, or `true` for catch-all) and a handler closure.
The handler receives the request and a context record.

The last route is the catch-all. `.static` looks for the request path in
the `static` folder, so `/style.css` serves `static/style.css`, with its
content type set from the file extension. If there is no such file, it
responds with a 404. The folder is relative to where you start http-nu, so
run it from the folder that holds `serve.nu`.

You can match on method, path, or both. For dynamic segments use
`path-matches`:

```nu
(route {path-matches: "/users/:id"} {|req ctx|
  $"User ID: ($ctx.id)"
})
```

## Step 4: The Store

Time to persist messages. http-nu embeds
[cross.stream](https://cross.stream), an append-only event store. Enable it
with `--store`, and add `-w` for watch mode so the server reloads when you
edit the script:

```bash
http-nu --store ./store :3001 -w serve.nu
```

```nu
use http-nu/html *
use http-nu/router *

def message-card [msg: record] {
  LI [(STRONG $msg.name) $" -- ($msg.message)"]
}

def page [messages: list] {
  HTML [
    (HEAD [
      (META {charset: "UTF-8"})
      (TITLE "Guestbook")
      (LINK {rel: "stylesheet" href: "/style.css"})
    ])
    (BODY [
      (H1 "Guestbook")
      (if ($messages | is-empty) {
        P "No messages yet. Be the first!"
      } else {
        UL { $messages | each {|m| message-card $m } }
      })
      (P (A {href: "/about"} "About this guestbook"))
    ])
  ]
}

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      let messages = .cat -T messages | get meta
      page $messages
    })

    (route {method: "POST" path: "/sign"} {|req ctx|
      from json | .append messages --meta $in
      "" | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: "GET" path: "/about"} {|req ctx|
      HTML [
        (HEAD [
          (META {charset: "UTF-8"})
          (TITLE "About")
          (LINK {rel: "stylesheet" href: "/style.css"})
        ])
        (BODY [
          (H1 "About")
          (P "A guestbook built with http-nu.")
          (P (A {href: "/"} "Back"))
        ])
      ]
    })

    (route true {|req ctx| .static "static" $req.path })
  ]
}
```

Add some messages:

```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"name":"Alice","message":"Hello!"}' localhost:3001/sign

curl -X POST -H "Content-Type: application/json" \
  -d '{"name":"Bob","message":"Great site!"}' localhost:3001/sign
```

Refresh the page -- your messages are there. Restart the server -- still
there. The store persists to `./store` on disk.

Notice `message-card` and `page`. In http-nu, reusable HTML fragments are
just Nushell `def` commands. Each returns an `{__html: ...}` record that
other tags accept as children without re-escaping. Composition is just
function calls.

## Step 5: Datastar -- Live Updates

Now for the payoff. [Datastar](https://data-star.dev) is a lightweight
hypermedia framework that connects your HTML to the server via server-sent
events. http-nu ships with a built-in Datastar SDK and serves the JS bundle
directly.

```bash
http-nu --datastar --store ./store :3001 -w serve.nu
```

```nu
use http-nu/html *
use http-nu/router *
use http-nu/datastar *

def message-card [msg: record] {
  LI [(STRONG $msg.name) $" -- ($msg.message)"]
}

def page [messages: list] {
  HTML [
    (HEAD [
      (META {charset: "UTF-8"})
      (TITLE "Guestbook")
      (LINK {rel: "stylesheet" href: "/style.css"})
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
    ])
    (BODY [
      (H1 "Guestbook")
      (UL {id: "messages"} {
        $messages | each {|m| message-card $m }
      })
      (P (A {href: "/about"} "About this guestbook"))

      (H2 "Sign the Guestbook")
      (INPUT {type: "text" placeholder: "Your name" "data-bind:name": ""})
      (BR)
      (TEXTAREA {placeholder: "Your message" "data-bind:message": ""})
      (BR)
      (BUTTON {
        "data-attr:disabled": "!$name || !$message"
        "data-on:click": "@post('/sign')"
      } "Sign")

      # Open SSE connection on page load
      (DIV {"data-init": "@get('/feed', {openWhenHidden: true})"})
    ])
  ]
}

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      let messages = .cat -T messages | get meta
      page $messages
    })

    (route {method: "POST" path: "/sign"} {|req ctx|
      let signals = from datastar-signals $req
      .append messages --meta $signals
      # Clear the inputs
      {name: "" message: ""} | to datastar-patch-signals | to sse
    })

    (route {method: "GET" path: "/feed"} {|req ctx|
      .cat -T messages --follow --new
      | each {|frame|
        message-card $frame.meta
        | to datastar-patch-elements --selector "#messages" --mode append
      }
      | to sse
    })

    (route {method: "GET" path: "/about"} {|req ctx|
      HTML [
        (HEAD [
          (META {charset: "UTF-8"})
          (TITLE "About")
          (LINK {rel: "stylesheet" href: "/style.css"})
        ])
        (BODY [
          (H1 "About")
          (P "A guestbook built with http-nu.")
          (P (A {href: "/"} "Back"))
        ])
      ]
    })

    (route true {|req ctx| .static "static" $req.path })
  ]
}
```

Open `http://localhost:3001` in two browser tabs. Sign the guestbook in
one -- the message appears instantly in both. No page refresh.

Here is what is happening:

- **Reactive signals**: `data-bind` creates the `name` and `message`
  signals and two-way binds the inputs to them. `data-attr:disabled` keeps
  the button disabled until both have a value.
- **Server actions**: clicking the button runs `@post('/sign')`, which
  sends all signals to the server as JSON. No form is needed. The server
  reads them with `from datastar-signals`, stores the message, and responds
  with `to datastar-patch-signals` to clear the inputs.
- **Live feed**: `data-init` opens an SSE connection to `/feed`.
  New messages stream through `to datastar-patch-elements`, which tells
  Datastar exactly which DOM element to update and how (`append` mode).
  By default Datastar closes a `@get` connection while the tab is hidden
  and reopens it when the tab is visible again. `openWhenHidden: true`
  keeps it open, so a background tab doesn't miss messages.
- **Streaming**: `.cat -T messages --follow --new` is a long-lived stream.
  Each new entry flows through `each`, gets wrapped as an SSE event, and
  reaches the browser immediately.

No virtual DOM. No build step. Just HTML fragments streamed over SSE.
