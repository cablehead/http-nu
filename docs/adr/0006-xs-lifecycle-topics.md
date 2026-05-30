# xs Lifecycle Topics

Unified topic vocabulary and namespace for actor / service / action
lifecycles in xs, and the compaction algorithm that consumes it.

## Background

Today actor, service, and action each carry a different ad-hoc lifecycle
vocabulary, all sharing the app's topic namespace:

- Actor: `<name>.register` / `<name>.unregister` (in), `<name>.active` /
  `<name>.unregistered` (out). One `.unregistered` covers every stop
  reason, distinguished only by meta.
- Service: `<name>.spawn` / `<name>.terminate` (in), `<name>.running` /
  `<name>.stopped` / `<name>.parse.error` / `<name>.shutdown` (out).
  `.stopped` carries `meta.reason in {finished, error, terminate, update}`.
- Action: `<name>.define` / `<name>.call` (in), `<name>.ready` /
  `<name>.error` (out). `.error` overloads parse failure and runtime
  failure. No undefine path.

Each dispatcher's startup compaction logic differs in shape, not just in
topic strings, and the runtime's own lifecycle frames sit in the same
namespace as user data.

## Problem

1. **Hot-replace with parse error**. Service and action keep the *old*
   instance running live (correct), but their *historical compaction* is
   latest-wins: on restart, the broken `.spawn` / `.define` is what's
   "remembered" and the previously good version is lost. Actor is worse --
   the old instance self-terminates before the new one is validated, so a
   broken hot-replace kills both.

2. **`.stopped` overloads compaction semantics**. Today the service
   dispatcher ignores `.stopped` entirely, which lets a naturally-completed
   or runtime-errored service restart on boot. Contrary to "if it chose to
   stop, it stays stopped."

3. **No topic-level distinction between "stopping but coming back" and
   "stopping for good"**. Server shutdown and a user `term` produce stops
   with opposite restart behaviour, but in the current scheme they're
   separated only by emitting different topics with no shared axis a
   reader can grok.

4. **Runtime lifecycle frames share the user namespace**. To find "all
   actor lifecycle events," the runtime has to scan every frame in the
   store and filter by suffix -- O(stream), no index can help cheaply.
   This drives the historical-scan cost we measured at ~17us/frame ×
   110k frames per dispatcher start.

## Decision

### Namespace

Lifecycle frames live under `xs.` -- a namespace owned by the runtime.
User-chosen data topics stay where they are.

```
xs.actor.<name>.<event>         actor lifecycle
xs.service.<name>.<event>       service lifecycle
xs.action.<name>.<event>        action lifecycle
xs.module.<name>                module registration (replaces <name>.nu)

<name>.recv / .send / .out      app-level data, runtime injects nothing
<anything-not-xs.*>              app-owned, runtime ignores
```

Glance test: a topic starting with `xs.` is runtime-managed; everything
else is app data.

Runtime queries become pure prefix scans on the existing hierarchical
`idx_topic` index -- no new index keyspace, no suffix matcher, no
schema-version bump:

| Query | Prefix |
|---|---|
| all system events | `xs.` |
| all actor lifecycle, every actor | `xs.actor.` |
| one actor's lifecycle | `xs.actor.snapshot-actor.` |
| all modules | `xs.module.` |
| one module's history | `xs.module.game.` |

Dispatcher startup collapses from a full-stream scan to a prefix-scoped
read of its own namespace -- in practice a few hundred frames instead of
110k.

### Lifecycle vocabulary

Apply uniformly across actor, service, and action (action uses a subset).
`in` = user-appended, `out` = runtime-emitted. The `<event>` segments:

| Event | Dir | Meaning |
|---|---|---|
| `create` | in | user wants this thing running |
| `term` | in | user wants this thing stopped |
| `active` | out | runtime is up; `meta` points at the originating `create` |
| `parse.error` | out | source failed to parse; `meta` points at the originating `create` |
| `fin.error` | out | runtime crashed |
| `fin.ok` | out | task ran to natural completion |
| `fin.term` | out | exited because of `term` |
| `replaced` | out | exited because a newer `create` won (transient marker) |
| `stopped` | out | exited because of `xs.stopping` (server shutdown) |

The `fin.*` family means "terminal, will not restart." `replaced` and
`stopped` are outside the family because they describe stops that should
*not* affect restart: `replaced` because a successor is coming;
`stopped` because the server itself is coming back.

### Compaction algorithm

Track two slots per `<kind>.<name>`:

- `confirmed`: last `create` that emitted `active`.
- `pending`: latest `create` with no terminal ack yet.

State transitions:

| Frame | Effect |
|---|---|
| `create` | `pending = this` |
| `active(source=X)` | `confirmed = create-X`; clear `pending` if it points at X |
| `parse.error(source=X)` | clear `pending` if it points at X |
| `term` | clear both |
| `fin.*` (error / ok / term) | clear both |
| `replaced` | no effect |
| `stopped` | no effect |

At threshold:

```
if pending:    try pending; on parse-fail, fall back to confirmed
elif confirmed: start confirmed
else:          nothing to start
```

### What this gets right

- **Hot-replace race** (`create₁ → active₁ → create₂ → ???` and xs dies):
  on restart, `confirmed=create₁`, `pending=create₂`. Try `create₂`; on
  fail, fall back to `create₁`.
- **Hot-replace, broken replacement** (`parse.error₂` lands live):
  `pending` cleared, `confirmed=create₁` survives. Old version restarts on
  boot.
- **Hot-replace success during transition window**: `replaced` does not
  clear `confirmed`, so the fallback survives the brief gap between
  old's `replaced` and new's `active`. The replacement's `active`
  overwrites `confirmed` cleanly.
- **First create, never acked** (xs died before processing): `pending` set,
  `confirmed` empty. Try `pending`. If it succeeds, advance; if it fails,
  nothing to fall back to -- correct.
- **Server crash mid-run**: `confirmed` set, `pending` empty. Start
  `confirmed` -- service was running fine, server crash should resume.
- **Server shutdown**: `stopped` doesn't affect compaction; `confirmed`
  persists; service resumes on next boot.
- **User `term` while xs offline is impossible** (fjall is single-writer),
  but `term` appended in a prior live session clears both slots, so the
  thing stays down on next boot.

### Trade-off accepted

`fin.*` clears both slots: a `term` is treated as a successful stop even
if xs died before the `fin.term` ack landed. Acceptable -- a `term` in
the log is a clear user intent, and respecting it without waiting for
the ack matches what the user wanted.

### Action subset

Actions don't run long-lived tasks. The events they use:

- `create` (was `.define`), `term` (new -- adds the missing undefine),
  `active` (was `.ready`), `parse.error`, `fin.term` (on user undefine),
  `fin.replaced` (on re-define), `replaced` (transient).
- No `fin.ok` (actions don't naturally finish), no `fin.error` at the
  *lifecycle* level (per-invocation runtime errors stay on the app's
  per-call response topic, not in the action's lifecycle stream), no
  `stopped` (actions don't run during `xs.stopping`).

## Migration

One-shot rewrite. No compatibility shim, no double-writes.

A schema-version migration walks the stream once and rewrites every
lifecycle frame's topic to its new namespaced form, in place:

```
snapshot-actor.register      -> xs.actor.snapshot-actor.create
snapshot-actor.active        -> xs.actor.snapshot-actor.active
snapshot-actor.unregistered  -> xs.actor.snapshot-actor.fin.{term|error|ok}
                                (split by meta.reason / meta.error presence)
api.spawn                    -> xs.service.api.create
api.terminate                -> xs.service.api.term
api.stopped (reason=...)     -> xs.service.api.fin.{ok|error|term} or .replaced
api.shutdown                 -> xs.service.api.stopped
api.parse.error              -> xs.service.api.parse.error
greet.define                 -> xs.action.greet.create
greet.ready                  -> xs.action.greet.active
greet.error                  -> xs.action.greet.parse.error  (parse cases only)
game.nu                      -> xs.module.game
```

Same `idx_topic` entries get rewritten alongside. CAS bytes are
untouched.

After migration, the runtime only knows the new vocabulary -- any
remaining old-shape frames in a partially-migrated store would be ignored
as app data, which is incorrect, so the migration is mandatory and
atomic with the version bump.

## Consequences

- Three dispatchers share one compaction algorithm template; only the
  per-kind prefix differs.
- No new index keyspace, no suffix matcher: the existing
  `idx_topic_prefix_keys` hierarchical index serves every runtime query
  in O(matches).
- Dispatcher cold-start drops from "scan whole stream for lifecycle
  topics" to "prefix-scan `xs.<kind>.`". For our 110k-frame measurement
  this collapses the historical phase from ~1.9s to milliseconds.
- Action gains an undefine (`term`) and a real lifecycle vocabulary,
  closing the gaps from the audit (broken `.define` retried every boot;
  `.error` overloading parse + runtime failure).
- Migration is mandatory: stores with mixed-shape frames don't work.
  Released versions of xs are pre/post the migration; there is no
  in-between operating mode.
