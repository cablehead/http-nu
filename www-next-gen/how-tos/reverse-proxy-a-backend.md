# Reverse-proxy a backend

Put http-nu in front of an existing service to add routing, headers, or path
rewriting without touching it. `.reverse-proxy` forwards the request and returns
the backend's response.

## The whole thing

```bash
http-nu :3001 -c '{|req| .reverse-proxy "http://localhost:8080"}'
```

Every request is forwarded to the backend and its response comes straight back.
When `.reverse-proxy` is first in the closure, the original request body is
forwarded too.

## As an API gateway

Strip a public prefix before forwarding, so `/api/v1/users` reaches the backend
as `/users`:

```bash
http-nu :3001 -c '{|req|
  .reverse-proxy "http://localhost:8080" {
    strip_prefix: "/api/v1"
  }
}'
```

## Add headers, rewrite the query

The optional config record carries headers, host handling, and query rewrites:

```nushell
{|req|
  .reverse-proxy "http://backend:8080" {
    headers: { "X-API-Key": "secret123" }
    query: ($req.query | upsert "context-id" "smidgeons" | reject "debug")
  }
}
```

## Proxy some paths, serve others

http-nu is a full server, so proxy the API and answer everything else yourself:

```nushell
{|req|
  if ($req.path | str starts-with "/api/") {
    .reverse-proxy "http://localhost:8080" { strip_prefix: "/api" }
  } else {
    "hello from http-nu"
  }
}
```

See the [Reference](/reference/reverse-proxy) for every option.
