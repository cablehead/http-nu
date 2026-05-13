# Cross-stream (xs) shadows for CF -- port plan

The xs commands (`.append`, `.cat`, `.last`, `.bus pub`, `.bus sub`)
underlie three demos blocked on CF today: **templates**, **quotes**,
and **2048-gameplay**. Plus `stor` (in-memory SQLite for ad-hoc
tables) which is a separate track.

Upstream xs source (read-only reference, not a build dependency):
`/Users/apple/workspace/go/src/github.com/joeblew999/xs/`.

## Architectural commitment: xs frames ride on Workspace

We've already shipped `cloudflare-shell-workspace` -- a FileSystem
abstraction backed by DurableObject SQLite + R2 spill. **The xs port
sits on top of that**, not next to it. Specifically:

- `.append topic data` -> write `data` to `/.xs/<topic>/<scru128>.json`
- `.cat topic` -> `read_dir("/.xs/<topic>")` + sort by name (scru128
  is monotonic time-ordered, so file-name sort = chronological order)
- `.last topic` -> last entry in the above
- `.last topic --follow` -> register `on_change` listener on the
  Workspace (path filter on `/.xs/<topic>/*`); stream new frames as
  files appear

This means **one storage primitive on CF (Workspace) for everything
that needs persistent file-like data**: demo assets, .static bytes,
and now xs frames. No parallel SQL table, no parallel R2 prefix, no
new schema, no new migration story.

What we get for free from Workspace:
- R2 spill: frames > 1.5 MB are automatically stored in R2 (the
  inline-vs-spill threshold is already implemented).
- Per-user isolation via the existing `/u/<user>/` URL prefix model.
- Visibility: every frame is browsable via `_workspace/ls?path=/.xs/`
  and inspectable via `_workspace/cat`.
- `on_change` listener: already wired into Workspace; reused for
  `--follow` instead of building a new pub/sub layer.

Cost: ~1 KB JSON overhead per frame (vs raw bytes in an LSM). Fine
for event-log volumes. Revisit only if a workload demands it.

## Why xs upstream doesn't compile on wasm (and why we don't need it to)

Three backends, none wasm-shippable:

| xs uses                                 | Why it fails on wasm     | What we use instead |
|-----------------------------------------|--------------------------|---------------------|
| `fjall` (LSM kv)                        | disk + mmap              | Workspace files     |
| `cacache` (disk CAS)                    | filesystem               | Workspace files (R2 spill)  |
| `tokio::sync::broadcast` for `--follow` | runtime across requests  | Workspace `on_change` listener |

We don't need xs to compile for wasm at all -- the upstream xs repo
stays a pure desktop dependency. Our CF port re-implements just the
command surface on top of Workspace. No fork of xs.

## Frame model (mirror upstream's shape)

```rust
struct Frame {
    id: Scru128Id,              // monotonic time-ordered id
    topic: String,
    hash: Option<Integrity>,    // optional CAS pointer (we just store inline for v1)
    meta: Option<serde_json::Value>,
    ttl: Option<TTL>,
}
```

scru128 generation is wasm-compatible. The Workspace file's name
encodes the id; meta/hash/ttl go inside the JSON body.

Pin the upstream xs revision we're mirroring in a comment in the
implementation. Audit when xs releases a Frame-shape change. Same
discipline as how we tracked `@cloudflare/shell@0.3.6`.

## Per-demo scope (after this layering)

### templates demo (smallest -- ~half a day)

Uses only `.append`:
```nu
open ($templates_dir | path join topics/page.html) | .append page.html
```

What we need:
- `.append <topic> [--meta JSON]` shadow: reads bytes from `$in`,
  writes one Workspace file per call.
- `.cat <topic>` shadow: `read_dir` + sort + concat or yield records.
- A small `frame_path(topic, id)` helper.

No streaming. Pure file writes. Smallest entry point.

### quotes demo (~1 day on top of templates)

Adds `.last <topic> --follow`:
```nu
.last quotes --follow
```

Server-Sent Events of frames as they arrive. Implementation:
- `.last <topic>` (no follow): read the highest-id file in the topic dir.
- `.last <topic> --follow`: register a callback on Workspace's
  `on_change`. Filter for paths matching `/.xs/<topic>/*`. Each
  `Create` event yields one frame to the stream.

The follow loop is a Nu `generate` that pulls from the listener queue.
On CF, the per-request lifetime of the listener matches the SSE
connection -- when the connection closes, the listener is dropped.

Verify the listener fires across distinct requests within the same DO
(Workspace state persists; listener is per-instance and replaced per
fetch). Likely needs a small broker pattern: one persistent listener
inside the DO that fans out to all active SSE handlers.

### 2048 gameplay (~1 day on top of quotes)

`.bus pub` and `.bus sub` are the in-process bus (not xs, but same
streaming shape). With the broker pattern above, `.bus pub` becomes
"write to a transient topic"; `.bus sub` becomes "follow that topic".

Actually `.bus` doesn't need persistence -- it's fire-and-forget. So
EITHER:
- Build a separate in-memory broker (no Workspace involvement); OR
- Use Workspace with a `TTL::Ephemeral` flag that triggers post-emit
  deletion.

Recommend the second: keeps the storage model uniform.

### stor (~2 days, independent track)

`stor *` doesn't fit the file model -- it's SQL queries against
arbitrary ad-hoc tables. CF version: thin wrapper over
`worker::SqlStorage` with a separate table namespace. Plan exists at
[`../stor/README.md`](../stor/README.md).

This is the *only* storage primitive in addition to Workspace. Two
primitives total on CF:
1. Files (Workspace) -- everything file-shaped, including xs frames.
2. Tables (stor) -- when you actually need SQL queries.

## Layout

```
src/cf/nu/nu_command/xs/
  PLAN.md             this file
  README.md           (TBD) one-line summary per command file
  CLAUDE.md           (TBD) working rules: scru128, frame schema,
                      Workspace path scheme, listener discipline
  PORT_STATUS.md      (TBD) what's ported vs upstream
  frame.rs            (TBD) Frame struct + path helpers
  append.rs           (TBD) .append shadow
  cat.rs              (TBD) .cat shadow
  last.rs             (TBD) .last shadow (--follow uses on_change)
  bus.rs              (TBD) .bus pub / .bus sub
```

Wire into `cf/mod.rs::engine()` after the existing shadows, so they
win Nu's name lookup (matches the pattern used by `VfsLs` etc.).

## When to start

Templates is the smallest closed scope -- it's the validator that the
Workspace-as-substrate idea works. Half a day, gives you a working
demo. Quotes follows naturally. 2048 reuses the listener plumbing.

Don't start mid-session. Each piece deserves focused time. The work
itself is small but the design decisions (listener lifecycle, ephemeral
frames, broker pattern) need a clear head.

## Open questions

1. **Per-user vs default DO for frames.** Workspace already isolates
   per user via `/u/<user>/`. If we want quotes-board cross-user
   visibility, frames should live in the default DO. Recommend:
   default DO for v1, per-user as opt-in later.

2. **TTL enforcement on Workspace.** xs supports `TTL::Last(n)` and
   `TTL::Ephemeral`. Workspace doesn't enforce TTL today; we'd need a
   GC pass (DO alarm + scheduled cleanup). v1 ignores TTL; we get to
   it when it matters.

3. **on_change listener fan-out.** Need exactly-once-broker-per-DO
   for `--follow` -- otherwise every SSE connection installs its own
   listener and we duplicate writes. Small Rust pattern; design when
   implementing quotes.

4. **Schema drift vs upstream xs.** Mirror xs's Frame shape, pin the
   xs revision in a comment, audit on bumps. Same discipline as
   `@cloudflare/shell@0.3.6`.
