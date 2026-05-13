# Cross-stream (xs) shadows for CF -- port plan

The xs commands (`.append`, `.cat`, `.last`, `.bus pub`, `.bus sub`,
etc.) underlie three demos that are currently blocked on CF:
**templates**, **quotes**, and **2048-gameplay**. Plus `stor`, which
overlaps but is in-memory SQLite (not xs).

Upstream xs source:
[`/Users/apple/workspace/go/src/github.com/joeblew999/xs/`](https://github.com/joeblew999/xs).

## Why xs doesn't compile on wasm

Three backends, none wasm-shippable:

| xs uses | Why it fails on wasm | Replacement |
|---|---|---|
| `fjall` (LSM kv) | disk + mmap | `worker::SqlStorage` (DO SQLite) for frame index |
| `cacache` (disk CAS) | filesystem | R2 for content (or inline blobs in SQL for small) |
| `tokio::sync::broadcast` for live subscribers | needs a runtime that survives across requests | DurableObject WebSocket Hibernation, OR long-poll |

## Frame model (unchanged on CF)

```rust
struct Frame {
    id: Scru128Id,              // monotonic time-ordered id
    topic: String,
    hash: Option<Integrity>,    // -> CAS content
    meta: Option<serde_json::Value>,
    ttl: Option<TTL>,
}
```

scru128 generation is wasm-compatible (no fs/time dep we can't satisfy).

## Per-demo scope

### templates demo (smallest -- ~1 day)

Uses only `.append`:
```nu
open ($templates_dir | path join topics/page.html) | .append page.html
```

Boot-time seed of named templates. No follow.

Needed:
- `.append <topic> [--meta JSON]` reading bytes from stdin
- `.cat <topic>` (non-following) for reads
- A frames table in the DO's SQLite: `(id, topic, meta_json, hash)`
- CAS via R2 (or inline blobs if < threshold)

Pure SQL+R2 implementation, no streaming. Aligns with our existing
Workspace pattern.

### quotes demo (next -- ~3 days)

Uses follow:
```nu
.last quotes --follow
```

SSE stream of frames as they arrive. Needs:
- Everything from templates above
- Plus: a way for one DO request to publish a frame and another DO request to receive it as a stream

Options:
1. **DurableObject WebSocket Hibernation**: holds connections across requests, broadcasts via DO method calls.
2. **Long-poll**: cursor-based polling against the SQL frames table. Simpler, more requests.

Recommendation: start with long-poll for simplicity. Move to WS Hibernation when latency / volume matter.

### 2048 gameplay (~2 days)

Uses `.bus sub` -- not xs proper, but the in-process bus in
`src/bus.rs`. Same problem (streaming across requests), same fix
(DO-level broadcast).

If we build the DO follow infrastructure for quotes, 2048 reuses it.

### stor (~2 days, independent track)

`stor *` family wraps an in-memory SQLite for ad-hoc tables. CF
version: thin wrapper over `worker::SqlStorage`. Schema-isolated from
the Workspace table. Plan exists at
[`src/cf/nu/nu_command/stor/README.md`](../stor/README.md).

## Layout

```
src/cf/nu/nu_command/xs/
  PLAN.md             this file (the why, the scope, the cost)
  README.md           (TBD) one-line links to per-command modules
  CLAUDE.md           (TBD) the working rules, schema-compat with xs
  PORT_STATUS.md      (TBD) what's ported, what's not, divergence notes
  store.rs            (TBD) the frame table + R2 CAS layer
  append.rs           (TBD) .append shadow
  cat.rs              (TBD) .cat shadow
  last.rs             (TBD) .last shadow (no-follow first)
  ...
```

Wire into `cf/mod.rs` engine init the same way the existing nu_command
shadows are wired (after `add_custom_commands`, so they win name lookup).

## When to start

When a concrete demo blocker forces it. Templates is the smallest entry
point. Quotes pulls in follow. 2048 gameplay pulls in bus. None is
trivial; each is its own session.

## Open questions before starting

1. **Do we share frame storage with the Workspace table, or separate?**
   Sharing simplifies the DO's mental model (one table to think about);
   separating decouples lifecycles (delete a workspace, keep frames).
   Recommendation: separate -- frames are an additive feature.

2. **R2 spill threshold for frame content** -- same 1.5 MB as Workspace,
   or higher? Frames tend to be small (events); 1.5 MB is generous.

3. **Schema compatibility with upstream xs?** xs uses fjall, not SQL.
   No binary compat. But the Frame shape (id/topic/meta/hash/ttl) is
   stable; we should mirror it. If/when xs grows a CF backend upstream,
   we'd align then.
