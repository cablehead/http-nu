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

Every request currently hits the same handler. Let's add proper routing
with the built-in router module.

```nu
use http-nu/html *
use http-nu/router *

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      HTML [
        (HEAD [(META {charset: "UTF-8"}) (TITLE "Guestbook")])
        (BODY [
          (H1 "Guestbook")
          (P "No messages yet.")
          (P (A {href: "/about"} "About this guestbook"))
        ])
      ]
    })

    (route {method: "GET" path: "/about"} {|req ctx|
      HTML [
        (HEAD [(META {charset: "UTF-8"}) (TITLE "About")])
        (BODY [
          (H1 "About")
          (P "A guestbook built with http-nu.")
          (P (A {href: "/"} "Back"))
        ])
      ]
    })

    (route true {|req ctx|
      "Not found" | metadata set --merge {'http.response': {status: 404}}
    })
  ]
}
```

`dispatch` tests routes in order -- first match wins. Each `route` takes a
test (a record for matching, or `true` for catch-all) and a handler closure.
The handler receives the request and a context record.

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
    (HEAD [(META {charset: "UTF-8"}) (TITLE "Guestbook")])
    (BODY [
      (H1 "Guestbook")
      (if ($messages | is-empty) {
        P "No messages yet. Be the first!"
      } else {
        UL { $messages | each {|m| message-card $m } }
      })
    ])
  ]
}

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      let messages = try { .cat messages } catch { [] }
        | each { $in.meta }
      page $messages
    })

    (route {method: "POST" path: "/sign"} {|req ctx|
      from json | .append messages --meta $in
      "" | metadata set --merge {'http.response': {status: 204}}
    })
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
      (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})
    ])
    (BODY [
      (H1 "Guestbook")
      (UL {id: "messages"} {
        $messages | each {|m| message-card $m }
      })

      (H2 "Sign the Guestbook")
      (FORM {
        "data-signals": "{name: '', message: ''}"
        "data-on:submit.prevent": "@post('/sign')"
      } [
        (INPUT {
          type: "text"
          placeholder: "Your name"
          "data-bind:name": ""
          required: true
        })
        (BR)
        (TEXTAREA {
          placeholder: "Your message"
          "data-bind:message": ""
          required: true
        })
        (BR)
        (BUTTON {type: "submit"} "Sign")
      ])

      # Open SSE connection on page load
      (DIV {"data-on:load": "@get('/feed')"})
    ])
  ]
}

{|req|
  dispatch $req [
    (route {method: "GET" path: "/"} {|req ctx|
      let messages = try { .cat messages } catch { [] }
        | each { $in.meta }
      page $messages
    })

    (route {method: "POST" path: "/sign"} {|req ctx|
      let signals = from datastar-signals $req
      .append messages --meta $signals
      # Clear the form
      {name: "" message: ""} | to datastar-patch-signals | to sse
    })

    (route {method: "GET" path: "/feed"} {|req ctx|
      .cat messages --follow --new
      | each {|frame|
        message-card $frame.meta
        | to datastar-patch-elements --selector "#messages" --mode append
      }
      | to sse
    })
  ]
}
```

Open `http://localhost:3001` in two browser tabs. Sign the guestbook in
one -- the message appears instantly in both. No page refresh.

Here is what is happening:

- **Reactive signals**: `data-signals` declares client state (`name` and
  `message`). `data-bind` two-way binds the inputs to those signals.
- **Server actions**: `data-on:submit.prevent` intercepts the form and
  sends signals as JSON via `@post('/sign')`. The server stores the message
  and responds with `to datastar-patch-signals` to clear the form.
- **Live feed**: `data-on:load` opens an SSE connection to `/feed`.
  New messages stream through `to datastar-patch-elements`, which tells
  Datastar exactly which DOM element to update and how (`append` mode).
- **Streaming**: `.cat messages --follow --new` is a long-lived stream.
  Each new entry flows through `each`, gets wrapped as an SSE event, and
  reaches the browser immediately.

No client-side JavaScript. No virtual DOM. No build step. Just HTML
fragments streamed over SSE.
