# nu_plugin_push

Nushell plugin exposing Web Push (RFC 8030 + 8291 + 8292) to http-nu and xs.

Sync, no tokio. Wraps [`web-push-native`](https://docs.rs/web-push-native) +
`ureq`. Does not run inside a Cloudflare Worker -- push send stays on the
origin (Cloudflare in front for HTTPS is fine; the plugin runs server-side).

## Commands

### `push vapid generate`

Generate a fresh P-256 keypair. Stdout-only -- no filesystem writes.

```
> push vapid generate
{
  public_key: "BJ9...uncompressed-sec1-base64url..."   # for browser applicationServerKey
  private_key_pem: "-----BEGIN EC PRIVATE KEY-----..."  # PEM PKCS#8 form
  private_key_b64url: "...32-byte-scalar-b64url..."     # env-friendly form
}
```

Run once per environment. Store via mise + fnox (see `examples/push-demo`).

### `push send <payload>`

Send a Web Push notification. Reads VAPID secrets from env.

```
> $sub | push send "hello" --ttl 60 --urgency normal
{ endpoint: "https://...", status: 201, result: "delivered", retry_after: null, message: null }
```

Input is either a single PushSubscription record / JSON string, or a list of
them. List input produces a list of result records (sequential -- true
parallel + streaming is on the roadmap).

#### Flags

| Flag        | Default  | Notes                                            |
|-------------|----------|--------------------------------------------------|
| `--ttl`     | `60`     | HTTP TTL header. 0 = deliver immediately or drop |
| `--urgency` | unset    | `very-low` / `low` / `normal` / `high`           |
| `--topic`   | unset    | Push service replace-on-receive key              |

#### Result codes

| `result`            | When                                       | Action                       |
|---------------------|--------------------------------------------|------------------------------|
| `delivered`         | 2xx                                        | Done                         |
| `expired`           | 404 / 410                                  | Append `push.subscription.expired` event |
| `payload_too_large` | 413                                        | Reduce payload below 4KB     |
| `rate_limited`      | 429                                        | Wait `retry_after` seconds   |
| `invalid_vapid`     | 400 / 401 / 403                            | Check VAPID_SUBJECT + key    |
| `push_service_down` | 5xx, transport errors                      | Retry later                  |
| `other`             | unclassified                               | Inspect `status` + `message` |

### `push encrypt <subscription> <payload>`

Build the encrypted + VAPID-signed POST without sending. Lets you debug
crypto / VAPID config without burning real subscriptions.

```
> push encrypt $sub "hello"
{
  curl: "curl -X POST -H 'authorization: vapid t=...,k=...' ..."
  url: "https://..."
  headers: { authorization: "vapid t=...", content-encoding: "aes128gcm", ttl: "60", ... }
  body_hex: "01abcd..."
  body_len: 100
}
```

### `push subscription parse <json>`

Parse + structurally validate a PushSubscription JSON string. Also exercises
key decoding so you fail loudly on malformed `p256dh` / `auth`.

### `push subscription validate <subscription>`

Send a `TTL: 0, Urgency: very-low, body: <empty>` push. By spec, TTL:0 means
"deliver immediately or drop" -- no user-visible notification, but you get
back the HTTP status. Lets you garbage-collect dead subs on a schedule
without spamming users.

```
> push subscription validate $sub
{ endpoint: "...", reachable: true, vapid_accepted: true, status: 201, message: null }
```

## Environment variables

Read at runtime by `send` and `validate`. Set via mise+fnox in production.

| Variable                 | Required          | Notes                                       |
|--------------------------|-------------------|---------------------------------------------|
| `VAPID_PRIVATE_KEY_PEM`  | one of            | PEM form                                    |
| `VAPID_PRIVATE_KEY`      | one of            | URL-safe b64 of 32-byte scalar              |
| `VAPID_SUBJECT`          | yes               | `mailto:` or `https:` URL. Apple requires.  |
| `VAPID_PUBLIC_KEY`       | only for browsers | Served verbatim to JS via `/vapid-public-key`. The plugin itself doesn't need it. |

## Key rotation

**VAPID key rotation invalidates every existing subscription.** Browsers
cache the public key against `applicationServerKey` at subscribe time. Any
push signed with a different key will fail with `invalid_vapid` and the
browser will re-subscribe on next visit. Plan for a coordinated re-subscribe
window; don't rotate casually.

## Why not Cloudflare Workers?

`nu_plugin_push` uses `ureq` (blocking I/O + threads) and the standard
`web-push-native` crypto stack. Workers don't support blocking I/O or
arbitrary native crypto. Run the plugin on a normal origin behind Cloudflare
(orange-cloud DNS or named tunnel) -- the browser still talks to Cloudflare
over HTTPS, and Cloudflare proxies to your origin where the plugin lives.

If you need to send push from a Worker, you'd use a different stack
(workers-rs + a Worker-compatible WebPush implementation, plus VAPID signing
via the Web Crypto API). That's a separate effort, not this plugin.

## Testing

```
cargo test -p nu_plugin_push
```

Three unit tests in-tree:

- `vapid::tests::generate_produces_round_trippable_keypair`
- `send::tests::build_request_emits_signed_encrypted_post`
- `send::tests::dry_run_emits_curl_and_hex_body`

The full subscribe -> send -> notification flow is exercised in
`examples/push-demo/test/test.mjs` (Playwright across Chromium / WebKit /
Firefox).
