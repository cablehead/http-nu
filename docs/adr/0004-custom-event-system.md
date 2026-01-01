# Custom Event System Over Tracing

## Context

http-nu needs structured logging with:
- Rich request/response data (headers, query params, timing)
- Multiple output formats (human-readable terminal, JSONL)
- High throughput without blocking request handling
- Works with `cargo install` without special flags

Evaluated options:

| | **log** | **tracing** | **emit** | **fastrace** | **custom** |
|---|---------|-------------|----------|--------------|------------|
| **Structured data** | ❌ kv unstable since 2019 | ❌ valuable unstable since 2022 | ✅ Native serde | ❌ | ✅ |
| **Your structs/enums** | ❌ | ❌ | ✅ | ❌ | ✅ |
| **cargo install works** | ✅ | ❌ needs RUSTFLAGS | ✅ | ✅ | ✅ |
| **API complexity** | Minimal | High | Medium | Low | Minimal |
| **Stability** | Stable | Features stuck 3+ years | Stable | API unstable | N/A |

tracing's `valuable` feature requires `RUSTFLAGS="--cfg tracing_unstable"` for all downstream users. This broke `cargo install http-nu`.

## Decision

Custom event system with broadcast channel and dedicated handler threads:

```rust
pub enum Event {
    Request { request_id: Scru128Id, method: String, path: String, ... },
    Response { request_id: Scru128Id, status: u16, latency_ms: u64, ... },
    Complete { request_id: Scru128Id, bytes: u64, duration_ms: u64 },
    Started { address: String, startup_ms: u64 },
    // ...
}

// Non-blocking emit via broadcast channel
fn emit(event: Event) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(event);
    }
}

// Handlers run in dedicated threads
pub fn run_jsonl_handler(rx: broadcast::Receiver<Event>) {
    std::thread::spawn(move || { /* blocking_recv + serialize + write */ });
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

### JSONL Handler
- Dedicated thread with `blocking_recv()`
- BufWriter with idle flush (flush when channel empty)
- Sustains 24K+ requests/sec without drops

### Human Handler
- Dedicated thread with `blocking_recv()`
- Rate limited to ~10 requests/sec (human-readable pace)
- Skipped requests tracked, periodically prints `... skipped N requests`
- Once a request is shown, its full lifecycle (Request→Response→Complete) completes

## Rationale

- **Non-blocking emit**: Request path just sends to channel, never waits
- **Dedicated threads**: Serialization and I/O off the async runtime
- **Idle flush**: Responsive under low load, efficient under high load
- **Rate limiting for human**: No point showing 60K req/sec to humans
- **Complete lifecycle**: Skipping happens at Request level; shown requests always complete
- **No unstable features**: Works with plain `cargo install`
