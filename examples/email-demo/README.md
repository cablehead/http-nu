# email-demo

http-nu service that fronts `cf_email_worker`. Two halves:

- **Inbound:** `cf_email_worker`'s `#[event(email)]` POSTs parsed messages to
  `POST /webhooks/email/inbound` with `X-Signature: sha256=<hmac>`. This
  service verifies the HMAC (using `CF_EMAIL_WEBHOOK_HMAC_KEY`) and appends
  an `email.received` frame to xs.
- **Outbound:** `POST /send` (bearer-auth via `EMAIL_ADMIN_TOKEN`) accepts a
  JSON record matching `EmailRequest` and appends an `email.send.requested`
  frame. A long-lived xs handler in `lib/send.nu` tails that topic and
  dispatches each request through the `email send` plugin command, posting
  outcome frames back to xs.

See `docs/email-native/events.md` for the full topic schema and
`docs/email-native/TASKS.md` for the broader plan.

## Quickstart

```bash
# 1. One-time: generate auth + HMAC secrets, store in fnox keychain.
mise run email:secrets:generate

# 2. Store the sender domain you'll deploy under.
mise run email:worker:domain-set -- mail.example.com

# 3. Build everything.
mise run build email:plugin:build email:worker:check

# 4. Deploy the worker (you'll be prompted to authenticate wrangler).
mise run email:worker:deploy

# 5. After deploy, store the deployed URL + push runtime secrets.
mise run email:worker:url-set -- https://cf-email-worker.<acct>.workers.dev
mise run email:worker:secrets:put

# 6. Set the webhook URL (where the worker POSTs inbound mail).
mise run email:worker:webhook-set -- https://<your-http-nu>/webhooks/email/inbound

# 7. Wire DNS + Email Routing.
mise run email:dns:check               # observe what's there now
# Add the SPF / DMARC records via Cloudflare dashboard, then:
mise run email:routing:rule:apply      # creates the catch-all routing rule

# 8. Run the demo service.
mise run email-demo:serve
```

## Routes

| Method | Path                          | Purpose                                                   |
|--------|-------------------------------|-----------------------------------------------------------|
| GET    | `/`                           | Terse JSON status                                         |
| GET    | `/health`                     | Readiness probe                                           |
| POST   | `/webhooks/email/inbound`     | HMAC-verified inbound -> xs `email.received`              |
| POST   | `/send`                       | Bearer-auth -> xs `email.send.requested`                  |

## Try it

Send a test outbound email via the demo's `/send` endpoint:

```bash
curl -X POST http://localhost:8080/send \
  -H "Authorization: Bearer $EMAIL_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "to": "you@example.com",
    "from": "noreply@mail.example.com",
    "subject": "hello from email-demo",
    "text": "test body",
    "request_ref": "demo-1"
  }'
```

This appends `email.send.requested` to xs. To actually dispatch it, run the
send-on-event handler in a second terminal:

```bash
nu -c "use ./examples/email-demo/lib/send.nu *; dispatch_loop"
```

The handler tails `email.send.requested`, calls `email send`, and appends
an outcome frame (`email.send.delivered` / `email.send.failed` /
`email.send.rate_limited` / `email.send.daily_quota_exceeded`).

To watch the lifecycle live:

```bash
.cat --follow --topic email.send.requested
.cat --follow --topic email.send.delivered
.cat --follow --topic email.received
```

## Troubleshooting

- **`unauthorized` on `POST /send`** -- `EMAIL_ADMIN_TOKEN` not set or doesn't
  match the bearer token. Check `mise run email:secrets:generate` was run.
- **`unauthorized` on `POST /webhooks/email/inbound`** -- HMAC mismatch.
  Confirm `CF_EMAIL_WEBHOOK_HMAC_KEY` is identical between this service and
  the Worker. `mise run email:worker:secrets:put` pushes the same value to
  the Worker; if you regenerated locally, re-run it.
- **`E_SENDER_NOT_VERIFIED` outcome frames** -- the sender domain isn't
  approved by Cloudflare Email Service. Visit the dashboard at Email ->
  Email Service -> Domains. `mise run email:dns:check` flags missing DNS.
- **`E_RECIPIENT_NOT_ALLOWED`** -- you're not on a paid Workers plan. Free
  plan can only send to verified destination addresses. Upgrade or add the
  recipient as a verified destination.
- **`E_DAILY_LIMIT_EXCEEDED`** -- Cloudflare's account-standing-based soft
  limit. Don't auto-retry today; circuit-break and resume tomorrow.

## Security notes (read before production)

- Bearer-token auth on `/send` is single-secret; rotate via
  `mise run email:secrets:generate` and re-push with `email:worker:secrets:put`.
- HMAC verification uses an openssl-shellout + string-equality. The window
  for a timing attack is microseconds over the network and bounded by the
  demo's other auth surfaces; if this lands in production replace with a
  constant-time helper.
- `email.send.requested` xs frames contain the full request body including
  the email text. Set xs retention / encryption appropriate to your data.
- The Worker base64-encodes inbound MIME and includes it in the webhook
  body. Treat `email.received` content as untrusted input.
