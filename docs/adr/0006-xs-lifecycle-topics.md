# xs Lifecycle Topics

Unified topic vocabulary for actor / service / action lifecycles in xs, and
the compaction algorithm that consumes it.

## Background

Today actor, service, and action each carry a different ad-hoc lifecycle
vocabulary:

- Actor: `.register` / `.unregister` (in), `.active` / `.unregistered` (out).
  One `.unregistered` covers every stop reason, distinguished only by meta.
- Service: `.spawn` / `.terminate` (in), `.running` / `.stopped` /
  `.parse.error` / `.shutdown` (out). `.stopped` carries `meta.reason in
  {finished, error, terminate, update}`.
- Action: `.define` / `.call` (in), `.ready` / `.error` (out). `.error`
  overloads parse failure and runtime failure. No undefine path.

Each dispatcher's startup compaction logic differs in shape, not just in
topic strings.

## Problem

Three concrete gaps surface:

1. **Hot-replace with parse error**. Service and action keep the *old*
   instance running live (correct), but their *historical compaction* is
   latest-wins: on restart, the broken `.spawn` / `.define` is what's
   "remembered" and the previously good version is lost. Actor is worse --
   the old instance self-terminates before the new one is validated, so a
   broken hot-replace kills both.

2. **`.stopped` overloads compaction semantics**. Today the service
   dispatcher ignores `.stopped` entirely, which lets a naturally-completed
   or runtime-errored service restart on boot. That's contrary to the
   principle "if a service chose to stop, it stays stopped."

3. **No way to tell at the topic level "stopping but coming back" vs
   "stopping for good"**. Server shutdown (`xs.stopping`) and a user `term`
   produce stops that should have opposite restart behaviour, but in the
   current scheme they're separated only by emitting different topics
   (`.shutdown` vs `.stopped {terminate}`) -- there's no shared axis the
   reader can grok at the topic level.

## Decision

### Lifecycle vocabulary

Apply uniformly across actor, service, and action (action uses a subset).
`in` = user-appended, `out` = runtime-emitted.

| Topic | Dir | Meaning |
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

Track two slots per topic-root:

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
  `confirmed` -- the service was running fine, server crash should resume.
- **Server shutdown**: `stopped` doesn't affect compaction; `confirmed`
  persists; service resumes on next boot.
- **User `term` while xs offline is impossible** (fjall is single-writer),
  but `term` appended in a prior live session persists -- it clears both
  slots, so the service stays down on next boot.

### Trade-off accepted

Rule simplicity over absolute completeness: `fin.*` clearing both slots
is a single line. The cost is that a `term` clears `confirmed` even if
xs died before the `fin.term` ack landed -- meaning a half-processed
`term` is treated as a successful stop. Acceptable: a `term` in the log
is a clear user intent, and respecting it without waiting for the ack
matches what the user wanted.

### Action subset

Actions don't run long-lived tasks, so the relevant subset is:

- `create` (define), `term` (undefine -- currently missing, would need
  to be added), `active` (today's `.ready`), `parse.error`,
  `fin.term` (only on user undefine), `fin.replaced` (on re-define),
  `replaced` (on re-define if we keep the transient marker).

No `fin.ok` (actions don't naturally finish), no `fin.error` (action
runtime errors today land on `.error` per invocation, not as a terminal
lifecycle event), no `stopped` (actions don't run during `xs.stopping`).

## Consequences

- Three dispatchers share one compaction algorithm template; only the
  topic-string set is per-processor.
- The suffix-index v1 set becomes: `.create`, `.parse.error`, `.active`,
  `.term`, `.fin.error`, `.fin.ok`, `.fin.term`, `.replaced`, `.stopped`,
  `.nu`. Non-overlapping, longest-match matcher works cleanly.
- Migration is a non-trivial rename: every existing actor/service/action
  in a live store would need its lifecycle frames either translated or
  shimmed by a compatibility layer. Worth a separate migration ADR.
- Closes the action-can't-be-undefined and broken-define-retries-every-boot
  gaps as side effects, by giving action a real `term` and a `fin.*`
  vocabulary.
