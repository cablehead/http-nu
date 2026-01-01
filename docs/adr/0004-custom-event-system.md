# Custom Event System Over Tracing

## Context

http-nu needs structured logging with:
- Rich request/response data (headers, query params, timing)
- Multiple output formats (human-readable terminal, JSONL)
- Zero-copy where possible
- Works with `cargo install` without special flags

Evaluated options:

| | **log** | **tracing** | **emit** | **fastrace** | **custom** |
|---|---------|-------------|----------|--------------|------------|
| **Structured data** | ❌ kv unstable since 2019 | ❌ valuable unstable since 2022 | ✅ Native serde | ❌ | ✅ |
| **Your structs/enums** | ❌ | ❌ | ✅ | ❌ | ✅ |
| **Zero-copy possible** | N/A | Partial | ❌ serde serialization | ✅ | ✅ |
| **cargo install works** | ✅ | ❌ needs RUSTFLAGS | ✅ | ✅ | ✅ |
| **API complexity** | Minimal | High | Medium | Low | Minimal |
| **Stability** | Stable | Features stuck 3+ years | Stable | API unstable | N/A |

tracing's `valuable` feature requires `RUSTFLAGS="--cfg tracing_unstable"` for all downstream users. This broke `cargo install http-nu`.

## Decision

Roll our own:

```rust
pub enum Event<'a> {
    Request { request_id: Scru128Id, method: &'a str, path: &'a str, ... },
    Response { request_id: Scru128Id, status: u16, latency_ms: u64, ... },
    Complete { request_id: Scru128Id, bytes: u64, duration_ms: u64 },
    Started { address: &'a str, startup_ms: u64 },
    // ...
}

pub trait Handler: Send + Sync {
    fn handle(&self, event: Event<'_>);
}
```

Handlers: `JsonlHandler` serializes on demand, `HumanHandler` formats to terminal.

## Rationale

- **Zero-copy at emission**: Event variants borrow data, no allocation
- **Handler decides cost**: JsonlHandler serializes, HumanHandler formats directly
- **No unstable features**: Works with plain `cargo install`
- **~200 lines**: Simpler than tracing's subscriber system
- **Full control**: Output format, field selection, timing all customizable
