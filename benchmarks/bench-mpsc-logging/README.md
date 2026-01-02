# Benchmark: mpsc-logging

> **Note:** Run benchmarks only with this branch checked out:
> `git checkout bench-mpsc-logging`

Comparing mpsc channel vs broadcast channel for logging.

## Setup

- Event enum is owned (no Clone needed for mpsc)
- emit() sends to std::sync::mpsc::Sender (non-blocking)
- Handlers run in dedicated threads with blocking recv()
- No clone on receive (mpsc moves ownership)

## Run

```bash
nu run.nu          # run and display
nu run.nu --save   # run and save to results.nuon

# Manual testing against a running server
oha -z 5s -c 50 http://127.0.0.1:3001/
```

## Results

See `results.nuon`
