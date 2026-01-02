# Benchmark: broadcast-logging

> **Note:** Run benchmarks only with this branch checked out:
> `git checkout bench-broadcast-logging`

Baseline benchmark after switching from synchronous `Handler` trait to async
`tokio::sync::broadcast` channel.

## Setup

- Event enum is owned (no lifetimes), derives Clone
- emit() sends to broadcast::Sender (non-blocking)
- Handler runs in spawned task, receives via broadcast::Receiver
- Each receive clones the Event (broadcast semantics)

## Run

```bash
nu run.nu          # run and display
nu run.nu --save   # run and save to results.nuon

# Manual testing against a running server
oha -z 5s -c 50 http://127.0.0.1:3001/
```

## Results

See `results.nuon`
