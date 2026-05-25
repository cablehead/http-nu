# push-native — Web Push + Notifications + a2hs

Bring fully working web push notifications to any http-nu + xs project. Single
plugin (`nu_plugin_push`), single demo example, fnox/mise wired for VAPID
secrets, Cloudflare quick tunnel for zero-config phone testing.

## Design summary

- **`crates/nu_plugin_push`** — nushell plugin (~500 LOC Rust) wrapping
  `web-push-native` + `ureq`. Five commands: `vapid generate`, `send`,
  `encrypt --dry-run`, `subscription parse`, `subscription validate`.
- **`examples/push-demo/`** — full example showing PWA manifest, service
  worker, iOS state machine, xs subscription storage + projection,
  Cloudflare tunnel for HTTPS, Playwright E2E across three engines.
- **VAPID secrets** via fnox → mise env → plugin. Never via flags or files.
- **Lifecycle** as data: `push send` emits typed result records
  (`delivered` | `expired` | `rate_limited` | ...). `.nu` handlers branch
  on result, emit `push.subscription.expired` events. xs projection
  excludes expired endpoints from next fanout.

## Architecture (production flow)

```
browser subscribes
    ↓ POST /subscribe (origin check)
serve.nu appends push.subscription.added to xs
    ↓
[later] anything appends push.send.requested to xs
    ↓
xs handler tails push.send.requested
    ↓
(current_subs from lib/subs.nu) | push send $payload --parallel 16
    ↓ stream of results
each result → xs append push.send.delivered | expired | failed
    ↓ expired → xs append push.subscription.expired
        ↓ (next current_subs projection excludes it — loop closed)
```

## Browser support matrix

| Browser            | Works | Notes                                    |
|--------------------|-------|------------------------------------------|
| Chrome desktop     | ✅    | direct                                   |
| Edge desktop       | ✅    | direct                                   |
| Firefox desktop    | ✅    | direct                                   |
| Safari macOS 13+   | ✅    | direct (no a2hs required)                |
| Chrome Android     | ✅    | direct                                   |
| Firefox Android    | ✅    | direct                                   |
| Safari iOS 16.4+   | ✅    | **install-to-Home-Screen required**      |
| Older browsers     | ❌    | state machine shows `unsupported`        |

## Tasks

> Order is rough — Playwright tests and docs can interleave. Blocked-by
> shown for hard dependencies only.

### Foundation

- [x] **#1** Branch `push-native` off `main`
- [ ] **#2** Scaffold `crates/nu_plugin_push` (Cargo.toml, lib.rs, command stubs that compile)

### Plugin commands

- [ ] **#3** `push vapid generate` — P-256 keypair → record `{ public_key, private_key_pem, private_key_b64url }`. Stdout only.
- [ ] **#4** `push send` — single subscription path. Reads `VAPID_PRIVATE_KEY_PEM` + `VAPID_SUBJECT` from env. Per-endpoint `aud`. Classified `result` field. *(blocked by #2)*
- [ ] **#5** `push encrypt --dry-run` — emits curl + headers + body_hex. No network. *(blocked by #4)*
- [ ] **#17** `push subscription validate` — TTL:0 empty-body push, no user-visible notification. *(blocked by #4)*
- [ ] **#18** Stream-mode `push send` — accepts stdin stream, emits result stream, `--parallel N`. *(blocked by #4)*

### Example: push-demo

- [ ] **#6** Scaffold `examples/push-demo/` (directory layout: serve.nu, www/, test/, lib/, icons.svg, cloudflared.yml)
- [ ] **#7** `www/state.js` — isIOS/isStandalone state machine, debug panel behind `?debug=1`
- [ ] **#8** `www/sw.js` — install → `skipWaiting()`, activate → `clients.claim()`, push → showNotification (iOS-safe options), notificationclick → focus/open
- [ ] **#9** `www/manifest.json` + iOS meta tags (`apple-touch-icon`, `apple-mobile-web-app-capable`, theme-color)
- [ ] **#10** `serve.nu` handlers — `/` static, `/subscribe` (origin-checked → xs), `/unsubscribe`, `/send` (bearer-auth via `PUSH_ADMIN_TOKEN`), `/vapid-public-key`. `Cache-Control: no-cache` on `/sw.js`.
- [ ] **#22** `lib/subs.nu` — `current_subs` projection over add/expired/unsub events *(blocked by #11)*
- [ ] **#23** PWA icons via `resvg` CLI from two SVG sources (regular + maskable). Mise task `push-demo:icons`. *(blocked by #6)*

### xs schema

- [ ] **#11** `docs/push-native/events.md` — schema for `push.subscription.{added,expired,unsubscribed}` and `push.send.{requested,delivered,expired,failed,rate_limited}`. Lifecycle handler sketch.

### Secrets & deployment

- [ ] **#19** fnox.toml item `http-nu-push-vapid` + mise tasks `push:vapid:generate`, `push:vapid:public`, `push-demo:serve` (injects env from fnox). *(blocked by #3)*
- [ ] **#21** VAPID `subject` collection — `push:vapid:generate` reads `PUSH_VAPID_SUBJECT` env (or prompts), stores alongside keypair.
- [ ] **#20** Quick-tunnel + QR dev task — `mise run push-demo:dev` starts cloudflared quick tunnel, captures `*.trycloudflare.com` URL, prints with `qrencode -t ANSIUTF8` for phone scanning.
- [ ] **#12** `cloudflared.yml` + README — named tunnel config for production, alongside the quick-tunnel dev path.

### Testing

- [ ] **#16** VAPID/encrypt golden fixture (Rust unit test in plugin). *(blocked by #4)*
- [ ] **#14** Playwright E2E harness — `examples/push-demo/test/test.mjs`, mirrors cedar-admin pattern. Chromium + WebKit + Firefox: subscribe → capture sub → POST /send → assert SW push received → assert notificationclick. *(blocked by #6, #8, #10)*
- [ ] **#15** Playwright iOS state-machine test — UA + `navigator.standalone` overrides, asserts each state machine branch renders correct banner. *(blocked by #7)*
- [ ] **#26** Negative test — malformed subscription POST → 400 with structured error. *(blocked by #14)*
- [ ] **#13** Manual iPhone smoke test — final 5-min gate on real hardware. *(blocked by #14)*

### Docs

- [ ] **#25** Plugin README (`crates/nu_plugin_push/README.md`) — standalone docs, env vars, command reference, result codes, key rotation warning, "doesn't run in Workers" note. *(blocked by #2)*
- [ ] **#24** Demo README (`examples/push-demo/README.md`) — 5-step quickstart ending in phone notification + troubleshooting keyed to state machine + security notes (endpoint sensitivity, xs at-rest, VAPID rotation, multi-device subs, CSRF, retention). *(blocked by #20, #22, #23)*

## Out of scope (deliberately)

- Native push outside the browser (APNs/FCM direct — different problem)
- Background sync, geofencing, rich-media notifications beyond minimal
- Topic-based fanout at >1M-sub scale (we POST per-subscription)
- Plugin running inside a Cloudflare Worker (ureq + threads — push send stays on origin)
- Per-user / per-account authn beyond the demo's bearer token

## Reference

- [web-push-native docs.rs](https://docs.rs/web-push-native/latest/web_push_native/)
- RFC 8030 (Generic Event Delivery Using HTTP Push)
- RFC 8291 (Message Encryption for Web Push)
- RFC 8292 (VAPID)
- iOS 16.4 release notes (web push, install-required)
