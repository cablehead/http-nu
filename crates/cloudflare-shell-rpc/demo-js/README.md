# cloudflare-shell-rpc-demo-js

A JS Worker that consumes `cloudflare-shell-rpc` via a service binding
and exposes curl-able HTTP routes that exercise every RPC method.
Serves two purposes:

1. **JS-consumer reference.** Shows the wrangler.toml service-binding
   declaration + the `env.SHELL_FS.method({...})` call pattern. One
   `wrangler.toml`, one `index.js`, no build step.
2. **Integration test.** `cf:fs:smoke` curls these routes in a
   round-trip sequence (write -> read -> stat -> list -> rm -> 404)
   to prove the wire works end-to-end.

## Minimal consumer snippet

The smallest thing you need in a different Worker on the same account
to use FS-RPC is **two lines of `wrangler.toml`** and a few lines of
`index.js`. No npm install, no build step.

`wrangler.toml`:

```toml
services = [
  { binding = "SHELL_FS", service = "cloudflare-shell-rpc" }
]
```

`index.js`:

```js
export default {
  async fetch(request, env) {
    // Write bytes.
    await env.SHELL_FS.writeFile({
      namespace: "alice",
      path: "/notes.md",
      data: btoa("hello, world"),     // bytes travel base64-encoded
      mimeType: "text/markdown",
    });

    // Read them back. resp.data is base64 (null if missing).
    const resp = await env.SHELL_FS.readFile({
      namespace: "alice",
      path: "/notes.md",
    });
    if (resp.data == null) return new Response("not found", { status: 404 });
    return new Response(atob(resp.data), {
      headers: { "content-type": "text/markdown" },
    });
  },
};
```

Other methods follow the same shape:

```js
await env.SHELL_FS.stat({ namespace, path });
await env.SHELL_FS.list({ namespace, path });
await env.SHELL_FS.mkdir({ namespace, path, recursive: true });
await env.SHELL_FS.rm({ namespace, path, recursive: true, force: true });
```

Namespaces must match `/^[a-zA-Z][a-zA-Z0-9_]*$/` -- the server
enforces this and an invalid namespace throws on the JS side. See
[`../types`](../types) for the full wire shape of every method.

## Routes

| Method | Path                    | RPC call          |
|--------|-------------------------|-------------------|
| GET    | `/`                     | (banner)          |
| GET    | `/fs/:ns/:path`         | `readFile`        |
| PUT    | `/fs/:ns/:path`         | `writeFile`       |
| DELETE | `/fs/:ns/:path`         | `rm`              |
| GET    | `/stat/:ns/:path`       | `stat`            |
| GET    | `/list/:ns/:path`       | `list`            |
| POST   | `/mkdir/:ns/:path`      | `mkdir`           |

Bytes in / out of `/fs/...` are raw on the HTTP side; the JS demo
base64-encodes / decodes around the RPC call (wire-side bytes travel
base64-encoded per the `cloudflare-shell-rpc-types` contract).

## Build / deploy

```bash
mise run cf:fs:demo:dev     # wrangler dev on :8789 (server must be on :8788)
mise run cf:fs:demo:deploy  # wrangler deploy
```

## Smoke test

```bash
mise run cf:fs:smoke        # local (defaults to http://127.0.0.1:8789)
LIVE_BASE=https://cloudflare-shell-rpc-demo-js.<sub>.workers.dev mise run cf:fs:smoke
```
