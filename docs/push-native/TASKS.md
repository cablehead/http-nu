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
- [x] **#2** Scaffold `crates/nu_plugin_push` (Cargo.toml, src/main.rs, 5 command stubs, registered in workspace, builds clean)

### Plugin commands

- [x] **#3** `push vapid generate` — P-256 keypair → record `{ public_key, private_key_pem, private_key_b64url }`. Stdout only. Test: `vapid::tests::generate_produces_round_trippable_keypair`.
- [x] **#4** `push send` — single + batch-list path. Reads `VAPID_PRIVATE_KEY_PEM` (or `VAPID_PRIVATE_KEY` b64url) + `VAPID_SUBJECT` from env. Per-endpoint `aud` handled by web-push-native. Classified `result` field (delivered / expired / payload_too_large / rate_limited / invalid_vapid / push_service_down / other). Test: `send::tests::build_request_emits_signed_encrypted_post`.
- [x] **#5** `push encrypt` — emits curl + headers + body_hex. No network. Test: `send::tests::dry_run_emits_curl_and_hex_body`. *(Note: dropped `--dry-run` flag — the command's only purpose is the dry-run output; redundant.)*
- [x] **#17** `push subscription validate` — TTL:0 empty-body push, no user-visible notification. Returns `{ endpoint, reachable, vapid_accepted, status, message? }`.
- [ ] **#18** Stream-mode `push send` — **PARTIAL**: batch list input (list in → list out, sequential) is in. **TODO**: true streaming (PluginCommand) + `--parallel N` concurrency via `std::thread::scope`. Not blocking demo.

### Example: push-demo

- [x] **#6** Scaffold `examples/push-demo/` (directory layout: serve.nu, www/, test/, lib/, icons.svg, cloudflared.yml)
- [x] **#7** `www/state.js` — isIOS/isStandalone state machine, debug panel behind `?debug=1`. Verified in browser preview: state computes correctly, debug panel shows isIOS=false, supported=true, etc.
- [x] **#8** `www/sw.js` — install → `skipWaiting()`, activate → `clients.claim()`, push → showNotification (iOS-safe options), notificationclick → focus/open
- [x] **#9** `www/manifest.json` + iOS meta tags (`apple-touch-icon`, `apple-mobile-web-app-capable`, theme-color)
- [x] **#10** `serve.nu` handlers — `/`, `/manifest.json`, `/sw.js` (no-cache), `/state.js`, `/icons/*`, `/health`, `/vapid-public-key`, `/subscribe` (origin-checked + plugin-validated → xs), `/unsubscribe`, `/send-self`, `/send` (bearer-auth). E2E tested: subscribe → fanout → fake endpoint 404 → expired event → projection excludes. Full reactive lifecycle.
- [x] **#22** `lib/subs.nu` — `current_subs` projection over add/expired/unsub events. Tested.
- [x] **#23** PWA icons via `resvg` CLI from two SVG sources (regular + maskable). Mise task `push-demo:icons`. Bell icon (yellow on slate), 192/512/512-maskable/apple-touch-180.

### xs schema

- [x] **#11** `docs/push-native/events.md` — schema for `push.subscription.{added,expired,unsubscribed}` and `push.send.{requested,delivered,expired,failed,rate_limited}`. Lifecycle handler sketch.

### Secrets & deployment

- [x] **#19** fnox.toml entries + mise tasks `push:vapid:generate`, `push:vapid:public`, `push:vapid:admin-token`, `push-demo:serve` (injects env from fnox).
- [x] **#21** VAPID `subject` collection — `push:vapid:generate` reads `PUSH_VAPID_SUBJECT` env, stores in fnox alongside keypair.
- [x] **#20** Quick-tunnel + QR dev task — `mise run push-demo:dev` starts cloudflared quick tunnel, captures `*.trycloudflare.com` URL, prints with `qrencode -t ANSIUTF8` for phone scanning (falls back gracefully if qrencode missing).
- [x] **#12** `cloudflared.yml` for production (named tunnel). Quick-tunnel for dev lives in mise.

### Testing

- [ ] **#16** VAPID/encrypt golden fixture (Rust unit test in plugin). *(blocked by #4)*
- [ ] **#14** Playwright E2E harness — `examples/push-demo/test/test.mjs`, mirrors cedar-admin pattern. Chromium + WebKit + Firefox: subscribe → capture sub → POST /send → assert SW push received → assert notificationclick. *(blocked by #6, #8, #10)*
- [ ] **#15** Playwright iOS state-machine test — UA + `navigator.standalone` overrides, asserts each state machine branch renders correct banner. *(blocked by #7)*
- [ ] **#26** Negative test — malformed subscription POST → 400 with structured error. *(blocked by #14)*
- [ ] **#13** Manual iPhone smoke test — final 5-min gate on real hardware. *(blocked by #14)*

### Docs

- [x] **#25** Plugin README (`crates/nu_plugin_push/README.md`) — standalone docs, env vars, command reference, result codes, key rotation warning, "doesn't run in Workers" note.
- [x] **#24** Demo README (`examples/push-demo/README.md`) — 5-step quickstart ending in phone notification + troubleshooting keyed to state machine + security notes (endpoint sensitivity, xs at-rest, VAPID rotation, multi-device subs, CSRF, retention).

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
