# push-native xs event schema

All push-related state lives in xs as an event stream. The plugin
(`nu_plugin_push`) is stateless -- subscriptions, lifecycle, audit are all
expressed as appended frames.

## Topics

### `push.subscription.added`

A browser registered. Emitted from `POST /subscribe`.

| Field        | Type                  | Notes                                   |
|--------------|-----------------------|-----------------------------------------|
| content      | `string` (PushSubscription JSON) | Verbatim from `pushSubscription.toJSON()` |
| meta         | (none)                |                                         |

### `push.subscription.expired`

The push service returned 404/410 -- the subscription is dead. Emitted by
the `serve.nu` fanout handler when `push send` returns `result: "expired"`.

| Field            | Type     | Notes                          |
|------------------|----------|--------------------------------|
| content          | (none)   |                                |
| meta.endpoint    | `string` | Unique key of the dead sub     |
| meta.last_status | `int`    | Optional; HTTP status observed |

### `push.subscription.unsubscribed`

User opted out via `POST /unsubscribe` (or in-browser `subscription.unsubscribe()`).

| Field            | Type     | Notes |
|------------------|----------|-------|
| content          | (none)   |       |
| meta.endpoint    | `string` |       |

### `push.send.requested`

Optional: append this when you want fanout-by-handler. A `.handler` tails
this topic and does the actual sending. Not used by the demo (which sends
inline from `POST /send-self` and `POST /send`); included for reference.

| Field          | Type     | Notes                                  |
|----------------|----------|----------------------------------------|
| content        | `string` | Payload string forwarded to subs       |
| meta.title     | `string` | Optional override                      |
| meta.tag       | `string` | Optional; service-worker dedupe by tag |
| meta.urgency   | `string` | very-low / low / normal / high         |

### `push.send.delivered` / `push.send.failed`

Audit log of send outcomes. Useful for dashboards / debugging without
re-running the fanout. Emitted by the handler that sends.

| Field            | Type     | Notes                                   |
|------------------|----------|-----------------------------------------|
| content          | (none)   |                                         |
| meta.endpoint    | `string` |                                         |
| meta.status      | `int`    | HTTP status from push service           |
| meta.result      | `string` | The `result` enum from `push send`      |
| meta.payload_len | `int`    | Bytes -- helps catch 413s before retry  |

## Projection: current subscriber list

The load-bearing projection. Folds add/expire/unsubscribe events into the
active subscriber list:

```nu
def current_subs [] {
  let added = (.cat -T push.subscription.added | each {|f|
    let sub = ($f.content | from json)
    { endpoint: $sub.endpoint, sub: $sub, at: $f.ts }
  })
  let removed = (
    (.cat -T push.subscription.expired
     | append (.cat -T push.subscription.unsubscribed))
    | each {|f| { endpoint: $f.meta.endpoint } }
    | get endpoint
    | uniq
  )
  $added
  | where {|x| $x.endpoint not-in $removed }
  | reduce -f {} {|x, acc| $acc | upsert $x.endpoint $x }
  | values | each {|x| $x.sub }
}
```

Idempotent. Re-subscribes after an expiry produce a fresh `added` event;
the reduce takes the latest, so the new keys win. The expired event from
before doesn't poison the new subscription.

See [`examples/push-demo/lib/subs.nu`](../../examples/push-demo/lib/subs.nu)
for the actual implementation including a `subs_table` admin view.

## Lifecycle in one diagram

```
browser POST /subscribe
    -> append push.subscription.added
                  |
                  v
   .--> [current_subs projection] -.
   |                                v
   |                          push send $payload
   |                                |
   |                  +--------+--------+
   |                  v        v        v
   |              delivered  expired  failed
   |                          |
   |                          v
   |   append push.subscription.expired
   |                          |
   '--------- excluded -------'

(re-subscribe after expiry simply appends another push.subscription.added;
the reduce in current_subs keeps the newest record.)
```

## Retention

Topics accumulate forever unless pruned. For production deployments:

- `push.send.*` audit topics can be capped by xs's `--ttl` retention if
  available, or periodically rolled into compacted summaries by a handler.
- `push.subscription.added` cannot be pruned freely -- you'd lose the
  current state. A future enhancement: compact by replaying into a snapshot
  topic.

For demo + dev, no retention is fine.
