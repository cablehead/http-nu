# email-native xs event schema

All email lifecycle state lives in xs as an event stream. The plugin
(`nu_plugin_email`) and Worker (`cf_email_worker`) are stateless --
deliveries, failures, inbound messages, and audit history are all
expressed as appended frames.

## Topics

### `email.send.requested`

A consumer wants to send an email. Emitted from anywhere in the app that
decides "send this now" -- auth flows, scheduled notifications, etc.

| Field                | Type     | Notes                                          |
|----------------------|----------|------------------------------------------------|
| content              | `string` (JSON) | Serialized EmailRequest: `{to, from, subject, text, html?, reply_to?, request_ref?}` |
| meta.request_ref     | `string` (optional) | Mirror of `content.request_ref` for cheap topic indexing |

Picked up by a `.handler` tailing this topic; the handler pipes records
into `email send`. See "Lifecycle" below.

### `email.send.delivered`

The Worker accepted the message and CF returned a `messageId`. Emitted by
the send-on-event handler from `email send`'s result records when
`result == "delivered"`.

| Field            | Type     | Notes                                          |
|------------------|----------|------------------------------------------------|
| content          | (none)   |                                                |
| meta.request_ref | `string` (optional) | Echoed from the request                       |
| meta.message_id  | `string` | CF Email Service message id                    |
| meta.request_id  | `string` (optional) | xs frame id of the originating `email.send.requested` |

### `email.send.failed`

Generic failure outcome -- everything that isn't rate-limited or
quota-exhausted. The plugin's `result` field is `"failed"` and an
`error_code` is attached. Common codes: `E_SENDER_NOT_VERIFIED`,
`E_RECIPIENT_NOT_ALLOWED`, `E_RECIPIENT_SUPPRESSED`, `E_INTERNAL_SERVER_ERROR`,
`E_BAD_REQUEST`, `E_TRANSPORT`, `E_UNKNOWN`.

| Field             | Type     | Notes                                          |
|-------------------|----------|------------------------------------------------|
| content           | (none)   |                                                |
| meta.request_ref  | `string` (optional)                              |
| meta.error_code   | `string` | One of the codes above                         |
| meta.message      | `string` | Upstream message (informational)               |
| meta.request_id   | `string` (optional) | xs frame id of the request                    |

### `email.send.rate_limited`

CF reports `E_RATE_LIMIT_EXCEEDED`. Retryable after `retry_after` seconds.

| Field             | Type     | Notes                                          |
|-------------------|----------|------------------------------------------------|
| content           | (none)   |                                                |
| meta.request_ref  | `string` (optional)                              |
| meta.retry_after  | `int`    | Seconds to wait                                |
| meta.request_id   | `string` (optional) | xs frame id of the request                    |

### `email.send.daily_quota_exceeded`

CF reports `E_DAILY_LIMIT_EXCEEDED`. Account-level cap; circuit-break sends
for the rest of the day rather than retrying.

| Field             | Type     | Notes                                          |
|-------------------|----------|------------------------------------------------|
| content           | (none)   |                                                |
| meta.request_ref  | `string` (optional)                              |
| meta.request_id   | `string` (optional)                              |

### `email.received`

An inbound message arrived via CF Email Routing. Emitted by `serve.nu`
after verifying the HMAC signature on the Worker's webhook POST.

| Field                  | Type     | Notes                                          |
|------------------------|----------|------------------------------------------------|
| content                | `string` | Raw RFC 5322 MIME, base64-encoded              |
| meta.envelope_from     | `string` | SMTP MAIL FROM                                 |
| meta.envelope_to       | `string` | SMTP RCPT TO (the address routed to us)        |
| meta.from              | `string` (optional) | Header `From:`                                |
| meta.to                | `string` (optional) | Header `To:`                                  |
| meta.subject           | `string` (optional) | Header `Subject:`                             |
| meta.message_id        | `string` (optional) | Header `Message-ID:` -- stable dedupe key     |
| meta.in_reply_to       | `string` (optional) | Header `In-Reply-To:` -- threading            |
| meta.received_at_ms    | `int`    | Worker receive time, epoch milliseconds        |

Downstream handlers parse the base64 content with whatever MIME library
suits them; the Worker stays parser-free.

## Lifecycle in one diagram

```
app code appends email.send.requested (content = JSON request)
                  |
                  v
   .handler tails email.send.requested
                  |
                  v
   record | email send
                  |
                  v
        POST {worker}/send (Bearer auth)
                  |
        +---------+---------+---------+---------+
        v         v         v         v         v
    delivered  failed  rate_limited daily_quota  (transport)
        |         |         |              |
        v         v         v              v
  email.send.{delivered|failed|rate_limited|daily_quota_exceeded}


external sender -> CF MX -> Email Routing -> cf_email_worker
                                                  |
                                                  v
                                  POST {http-nu}/webhooks/email/inbound
                                                  |
                                                  v
                                  serve.nu verifies HMAC
                                                  |
                                                  v
                                  append email.received (content = raw MIME b64)
```

## Idempotency

- `request_ref` is the **caller's** correlation key. It's not unique on its
  own; the originating `email.send.requested` xs frame id is the
  unique key for "this send attempt". `meta.request_id` carries that
  forward into the outcome events.
- For inbound: `meta.message_id` (the RFC 5322 `Message-ID:` header) is the
  stable dedupe key. A retry of the same inbound message will appear with
  the same `message_id`. Consumers that need exactly-once should track
  seen `message_id` values.
- `email send` does not auto-retry rate-limited or quota-exhausted
  attempts. The xs handler should decide -- e.g., re-append
  `email.send.requested` after `retry_after` seconds, or shelf and ask
  human.

## Retention

- `email.send.requested` cannot be pruned freely without losing the audit
  trail of "what we tried to send." Roll into a compacted summary if size
  matters.
- `email.send.{delivered,failed,rate_limited,daily_quota_exceeded}` --
  outcome topics. Useful for dashboards. TTL with xs retention if available.
- `email.received` -- inbound history. Same constraint as
  `email.send.requested`: pruning loses audit.

For demo + dev, no retention is fine.
