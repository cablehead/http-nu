# Serve a single-page app

A single-page app ships one `index.html` and lets the client router handle the
rest. The catch with static serving is deep links: a request for `/dashboard`
has no file on disk, so it 404s on refresh. http-nu's `.static` has a
`--fallback` for exactly this.

## Static files

Serve a directory, mapping the request path to a file:

```bash
http-nu :3001 -c '{|req| .static "./dist" $req.path}'
```

`.static` infers the content type from the file extension and ignores anything
else the closure returns.

## The SPA fallback

Add `--fallback` so any path that does not match a file serves your app shell
instead of returning 404:

```bash
http-nu :3001 -c '{|req| .static "./dist" $req.path --fallback "index.html"}'
```

Now `/`, `/dashboard`, and `/users/42` all return `index.html`, the client
router takes it from there, and real assets like `/app.js` and `/style.css`
still serve directly.

## Mixing with an API

Routes compose, so serve the app and an API from one closure:

```nushell
use http-nu/router *

{|req|
  dispatch $req [
    (route {path-matches: "/api/health"} {|req ctx| {ok: true} })
    (route true {|req ctx| .static "./dist" $req.path --fallback "index.html" })
  ]
}
```

API routes win; everything else falls through to the app shell. See the
[Reference](/reference#serving--operations) for the rest of `.static`.
