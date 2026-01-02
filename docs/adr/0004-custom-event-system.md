# Custom Event System

## Context

http-nu needs structured logging:

- Request/response lifecycle (headers, timing, bytes)
- Two output formats: human-readable terminal, JSONL for tooling
- High throughput without blocking request handling
- Works with `cargo install` (no special flags)

## Options

**tracing**: Rich ecosystem, but `valuable` feature (needed for custom structs)
requires `RUSTFLAGS="--cfg tracing_unstable"`. Breaks `cargo install`. Rejected.

**emit**: Viable. Native serde, stable, works with cargo install. Provides
emit_term (human) and emit_file (rolling JSONL to files). However:

- emit_file targets files, not stdout
- We'd need a custom emitter for stdout JSONL anyway
- Rate-limiting for human output not built-in
- Adds dependency for ~300 lines of purpose-built code

**custom**: Typed Event enum, broadcast channel, dedicated handler threads.
Simple, fits exact requirements.

## Decision

Custom event system. Typed events, broadcast to handler threads:

```rust
pub enum Event {
    Request { request_id: Scru128Id, request: Box<RequestData> },
    Response { request_id: Scru128Id, status: u16, latency_ms: u64, ... },
    Complete { request_id: Scru128Id, bytes: u64, duration_ms: u64 },
    Started { address: String, startup_ms: u64 },
    // ...
}

fn emit(event: Event) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(event); // non-blocking
    }
}
```

## Architecture

```
Request Path                    Handler Thread
     │                               │
  emit() ──► broadcast::send() ──► blocking_recv()
     │         (non-blocking)        │
     ▼                               ▼
  continue                     serialize + write
```

**JSONL handler**: Dedicated thread, BufWriter, flushes when channel empty. 24K+
req/sec sustained.

**Human handler**: Dedicated thread, rate-limited to ~10 req/sec. Tracks skipped
requests. Once shown, a request's full lifecycle completes.

## Tradeoffs

- Events are cloned at emit (headers→HashMap, IPs→String). Acceptable at current
  throughput.
- No file rotation, OTLP, or other emit features. Not needed—stdout only.
- ~300 lines to maintain vs external dependency.
