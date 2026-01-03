# Live Quotes Example

Demonstrates Datastar SSE with live updates using the embedded cross.stream store.

## Run

```bash
cd examples/quotes
http-nu :3002 --store ./store ./serve.nu
```

Visit http://localhost:3002

## Test

In another terminal, add quotes via curl:

```bash
curl -X POST -d '{"quote": "Stay hungry, stay foolish.", "who": "Steve Jobs"}' http://localhost:3002/
```

Or via the store's Unix socket directly:

```bash
curl --unix-socket ./store/sock -X POST \
  -H "xs-meta: $(echo '{"quote": "Be the change.", "who": "Gandhi"}' | base64)" \
  http://localhost/quotes
```

The page updates in real-time as quotes are added.
