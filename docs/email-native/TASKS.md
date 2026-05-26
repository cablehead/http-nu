# email-native -- Cloudflare Email Service (send + receive)

Bring transactional email to any http-nu + xs project, via Cloudflare Email
Service. Send magic links and notifications; receive inbound mail as xs
events. Beta product; Workers Paid plan; arbitrary recipients confirmed
working in practice.

## Working location

This branch (`email-native`) lives in a separate git worktree:
`/Users/apple/workspace/go/src/github.com/joeblew999/http-nu-email-native/`
Sister to the main checkout. Branched off `origin/main` to stay clear of
in-flight WIP on other branches (`push-native`, `cedar-native`).

## Design summary

- **`crates/nu_plugin_email`** -- nushell plugin (~400 LOC Rust). Thin HTTPS
  client to our own CF Worker. Three commands: `send` (stream-mode in/out),
  `send --dry-run`, `config show`. No MIME assembly here; no WASM concerns
  here.
- **`crates/cf_email_worker`** -- workers-rs Rust Worker. Two handlers in one
  binary: `#[event(fetch)]` answers `POST /send` from the plugin and uses
  the `[[send_email]]` binding to call CF Email Service;
  `#[event(email)]` receives inbound mail from CF Email Routing and POSTs a
  parsed payload to a configured http-nu webhook URL.
- **Two top-level crates**, not nested. Matches the only committed
  precedent in this repo (`crates/nu_plugin_push`). Avoids coupling to the
  uncommitted `crates/cloudflare-shell-rpc/` layout still being shaped on
  another branch.
- **`examples/email-demo/`** -- full example. `serve.nu` handles
  `/webhooks/email/inbound` (HMAC-verified, appends `email.received` to xs),
  exposes `/send` admin endpoint, ships a send-on-event handler that tails
  `email.send.requested` and pipes records into `email send`.
- **Secrets** via fnox -> mise env -> plugin and Worker. One fnox item
  `http-nu-email-cf` holds everything (see Secrets section).
- **Lifecycle** as data: `email send` emits typed result records
  (`delivered` | `rate_limited` | `daily_quota_exceeded` |
  `sender_not_verified` | `recipient_not_allowed` | `failed`). `.nu`
  handlers branch on result and append `email.send.{outcome}` events.

## Architecture (production flow)

```
Outbound:
    anything appends email.send.requested to xs
        |
    xs handler tails email.send.requested
        |
    records | email send --parallel 8
        |
    plugin POSTs {from, to, subject, text, html?, headers?} to Worker /send
    with `Authorization: Bearer $CF_EMAIL_AUTH_TOKEN`
        |
    Worker validates bearer, assembles MIME via SendEmailBuilder
        |
    Worker calls env.send_email("EMAIL").send_with_builder(&b)
        |
    each result -> xs append email.send.{delivered|failed|...}

Inbound:
    external sender -> MX -> CF Email Routing
        |
    dashboard rule: *@$CF_EMAIL_SENDER_DOMAIN -> cf_email_worker
        |
    Worker #[event(email)] parses ForwardableEmailMessage
        |
    Worker POSTs {from, to, subject, text, html?, raw_mime, received_at}
    to $CF_EMAIL_WEBHOOK_URL with `X-Signature: hmac-sha256(key, body)`
        |
    serve.nu verifies HMAC, appends email.received to xs
        |
    any consumer (.cat / .tail) reads and reacts
```

## CF Email Service constraints

| Concern              | Value                                                |
|----------------------|------------------------------------------------------|
| Status               | Beta -- APIs may change before GA                    |
| Plan                 | Workers Paid required                                |
| Free included        | 3,000 messages / month                               |
| Overage              | $0.35 per 1,000 messages                             |
| Recipient rule       | Any external address on paid plan (confirmed)        |
| Sender rule          | Domain on Cloudflare with DKIM verified              |
| Message size         | 5 MiB (25 MiB to verified addresses)                 |
| Recipients per msg   | 50 (to + cc + bcc)                                   |
| Daily cap            | "Account-standing based" -- unpublished, soft limit  |
| Worker invocation    | 50 ms CPU, 50 subrequests per request                |

Error codes we map to result records: `E_RATE_LIMIT_EXCEEDED`,
`E_DAILY_LIMIT_EXCEEDED`, `E_SENDER_NOT_VERIFIED`, `E_RECIPIENT_NOT_ALLOWED`.

## Inbound wiring (gotchas from receive-email example)

- **Inbound is NOT declared in `wrangler.toml`.** Unlike `[[send_email]]`,
  the email handler is wired through the dashboard (Email > Email Routing
  > Email Workers) or via the CF API. Deploy is two steps: wrangler ships
  the binary; route assignment is separate.
- **Local dev seam:** `wrangler dev --local` exposes
  `POST /cdn-cgi/handler/email` -- post a raw `.eml` body with
  `?from=&to=` query params to simulate an inbound. Use this for
  integration tests instead of cooking up a fake `ForwardableEmailMessage`.
- **`InboundEmail::reply()` requires DMARC** on the sender's domain.
  Missing `_dmarc` record at `_dmarc.<sender-domain>` is treated as a
  fail. Not on our critical path (we forward to webhook rather than
  replying), but flag it if we ever add auto-replies.

## xs event schema (sketch -- finalize in #11)

- `email.send.requested {request_ref, to, from, subject, text, html?, reply_to?, headers?, idempotency_key?}`
- `email.send.delivered {request_ref, message_id}`
- `email.send.failed {request_ref, error_code, retry_after?}`
- `email.send.rate_limited {request_ref, retry_after}`
- `email.send.daily_quota_exceeded {request_ref}`
- `email.received {message_id, from, to, subject, text, html?, raw_mime, received_at}`

## Secrets (fnox item `http-nu-email-cf`)

| Key                          | Used by         | Purpose                                                  |
|------------------------------|-----------------|----------------------------------------------------------|
| `CF_API_TOKEN`               | wrangler (deploy) | Deploy + manage the Worker. NOT used at runtime.       |
| `CF_EMAIL_AUTH_TOKEN`        | plugin + Worker | Bearer between `nu_plugin_email` and our Worker. Gates `POST /send` so only we can use the deployed Worker. |
| `CF_EMAIL_WEBHOOK_HMAC_KEY`  | Worker + serve.nu | HMAC-SHA256 key for Worker -> http-nu webhook signing. |
| `CF_EMAIL_WORKER_URL`        | plugin          | Where plugin POSTs (e.g., `https://email.<acct>.workers.dev`). |
| `CF_EMAIL_WEBHOOK_URL`       | Worker          | Where Worker POSTs inbound (e.g., `https://http-nu.example.com/webhooks/email/inbound`). |
| `CF_EMAIL_SENDER_DOMAIN`     | Worker + tasks  | Sender domain (e.g., `mail.example.com`). Switching domains is a fnox edit, not a code change. |

Per [[feedback_secrets_fnox_mise]] and [[feedback_cloud_project_per_repo]]:
dedicated fnox item, never hand-edited `.dev.vars`, never reused tokens
from another repo.

## Tasks

> Order is rough -- testing and docs can interleave. Blocked-by shown for
> hard dependencies only.

### Foundation

- [x] **#1** Branch `email-native` off `origin/main` (worktree at `../http-nu-email-native/`)
- [x] **#2** Scaffold `crates/nu_plugin_email` (Cargo.toml, lib.rs, command stubs that compile)
- [x] **#3** Scaffold `crates/cf_email_worker` (Cargo.toml, wrangler.toml with `[[send_email]] name = "EMAIL"`, lib.rs with stub `#[event(fetch)]` + `#[event(email)]`)
- [x] **#4** Workspace `members` updated in root `Cargo.toml` (with `default-members` excluding the WASM crate so `cargo build` works without a target flag)

### Worker

- [x] **#5** `POST /send` handler -- bearer-auth via `CF_EMAIL_AUTH_TOKEN` (constant-time compare), parses structured body, assembles MIME via `SendEmailBuilder`, calls `env.send_email("EMAIL").send_with_builder(&b).await`, classifies `worker::Error` variants into canonical error codes (RateLimit, DailyLimit, EmailRecipientNotAllowed, EmailRecipientSuppressed, InternalError, plus pass-through for `UnknownJsError { code: Some(_) }`).
- [x] **#6** `#[event(email)]` handler -- reads `ForwardableEmailMessage`, extracts envelope + headers + base64'd raw MIME, POSTs JSON to `CF_EMAIL_WEBHOOK_URL` with `X-Signature: sha256=<hmac>`. Worker stays MIME-parser-free; downstream parses if needed.
- [x] **#7** No bare `?` in handlers; `match` + `console_error!` throughout, per [[feedback_workers_rs_d1]].
- [ ] **#8** Local dev path -- `wrangler dev --local` writes outbound to `.eml`; inbound simulated via `POST /cdn-cgi/handler/email` with `.eml` body. Document both loops.

### Worker dependency note

- The `worker` crate is pinned to git rev `3d0903a` (commit that landed
  `#[event(email)]`). Latest crates.io release (`0.8.3`) predates that
  feature, so it can't be used until CF publishes a new minor. Revisit when
  they do.

### Plugin commands

- [x] **#9** `email send` -- POSTs to `$CF_EMAIL_WORKER_URL/send` with `Authorization: Bearer $CF_EMAIL_AUTH_TOKEN`. Accepts a single record or a list of records (sequential batch). Returns result records with classified `result` field via `Outcome::from_error_code`. `--parallel N` stream mode is a follow-up.
- [x] **#10** `email send --dry-run` -- emits a record `{ dry_run: true, curl, body, request_ref? }` (token masked). No network.
- [x] **#15** `email config show` -- prints env-var state, masks the auth token, exits with a `LabeledError` if a required env var is missing.

### xs schema

- [x] **#11** `docs/email-native/events.md` -- schema for `email.send.{requested,delivered,failed,rate_limited,daily_quota_exceeded}` and `email.received`, with lifecycle diagram, idempotency notes (request_ref vs frame id vs Message-ID), retention guidance.

### Example: email-demo

- [ ] **#12** Scaffold `examples/email-demo/` (serve.nu, www/, test/, lib/, README.md)
- [ ] **#13** `serve.nu` handlers -- `/webhooks/email/inbound` (HMAC-verified -> xs `email.received`), `/send` (bearer-auth via `EMAIL_ADMIN_TOKEN` -> xs `email.send.requested`), `/`, static. *(blocked by #12)*
- [ ] **#14** `lib/send.nu` -- xs handler that tails `email.send.requested` and pipes records into `email send --parallel 8`. Each result appended back to xs. *(blocked by #9, #11)*

### Secrets & deployment

- [x] **#16** `fnox.toml` -- CLOUDFLARE_API_TOKEN + CF_EMAIL_{AUTH_TOKEN,WEBHOOK_HMAC_KEY,WORKER_URL,WEBHOOK_URL,SENDER_DOMAIN} entries. Keychain item names repo-prefixed (`HTTP_NU_EMAIL_*`) per [[feedback_cloud_project_per_repo]].
- [x] **#17** Mise tasks -- `email:plugin:{build,test}`, `email:worker:{check,dev,deploy,tail}`, `email:secrets:generate`, `email:worker:{url-set,webhook-set,domain-set}`, `email:worker:secrets:put`. All deploy paths route through `fnox exec`.
- [ ] **#18** Sender domain setup script -- mise task that walks DNS for SPF / DKIM / DMARC additions; idempotent; reads target domain from `$CF_EMAIL_SENDER_DOMAIN`. `email-demo:serve` task lands with #12-#14.
- [ ] **#19** Email Routing rule setup -- mise task using `wrangler` (or CF API) to create `*@$CF_EMAIL_SENDER_DOMAIN -> cf_email_worker` route. Idempotent. Documents the dashboard alternative.

### Testing

- [ ] **#20** Plugin unit tests -- result classification covers all six outcome branches; HMAC round-trip; dry-run formatting golden. *(blocked by #9)*
- [ ] **#21** Worker integration test -- `wrangler dev --local` + plugin send -> assert `.eml` written; `POST /cdn-cgi/handler/email` simulated inbound -> assert webhook POST with valid HMAC. *(blocked by #5, #6)*
- [ ] **#22** xs flow E2E -- append `email.send.requested`, assert `email.send.delivered` appears within N ms (against `wrangler dev --local`). *(blocked by #14)*
- [ ] **#23** Manual paid-tier smoke -- one real send to a gmail account; one real inbound from a gmail account; recorded in `docs/email-native/smoke.md`. Final gate.

### Docs

- [ ] **#24** Plugin README (`crates/nu_plugin_email/README.md`) -- env vars, command reference, outcome codes, "doesn't run in Workers" note, idempotency guidance. *(blocked by #2)*
- [ ] **#25** Worker README (`crates/cf_email_worker/README.md`) -- wrangler.toml binding, secrets to set, sender + DKIM setup, Email Routing rule, beta caveat, DMARC-on-reply note. *(blocked by #3)*
- [ ] **#26** Demo README (`examples/email-demo/README.md`) -- quickstart, troubleshooting, security notes (HMAC rotation, bearer-token rotation, xs at-rest, inbound spam considerations). *(blocked by #14, #16)*

## Out of scope (deliberately)

- Resend / Postmark / SES fallback. CF Send only, per locked design. Re-open
  if beta proves unreliable in production.
- SMTP submission interface. CF Workers binding only.
- Auto-reply via `InboundEmail::reply()`. We forward to webhook instead;
  reply path can be added later if a use case appears (and after we sort
  the DMARC-on-sender requirement).
- Reputation management beyond reading `E_*` error codes.
- Bounce / complaint loop processing. Capture if CF surfaces it; otherwise
  defer.
- Plugin running inside a Cloudflare Worker -- plugin is host-side only.
- Multi-domain sender abstraction -- one verified sender domain per Worker
  deploy for now.

## Reference

- workers-rs send-email example: `/Users/apple/workspace/go/src/github.com/cloudflare/workers-rs/examples/send-email/`
- workers-rs receive-email example: `/Users/apple/workspace/go/src/github.com/cloudflare/workers-rs/examples/receive-email/`
- CF Email Service docs: <https://developers.cloudflare.com/email-service/>
- CF Email Service limits: <https://developers.cloudflare.com/email-service/platform/limits/>
- CF Email Service pricing: <https://developers.cloudflare.com/email-service/platform/pricing/>
- CF Email Routing (inbound): <https://developers.cloudflare.com/email-routing/email-workers/>
- CF reply-from-Worker DMARC requirement: <https://developers.cloudflare.com/email-routing/email-workers/reply-email-workers/#requirements>
- worker crate docs: <https://docs.rs/worker/latest/worker/>
- RFC 5322 (Internet Message Format)
- RFC 6376 (DKIM)
- RFC 7489 (DMARC)
