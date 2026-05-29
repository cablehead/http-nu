# 2048 over the Local Bus

A solo-tab 2048 demo that uses [`.bus pub` / `.bus sub`](../../README.md#local-bus)
end-to-end:

- Each keypress (`hjkl` / arrow keys / `r`) becomes a fire-and-forget POST.
- The handler publishes the impulse to a per-tab topic (`game.<tabid>.move`).
- A long-lived SSE consumer subscribes to `game.<tabid>.*`, runs the game loop
  in `generate`, and patches the board on every event.

State lives entirely in the SSE consumer's `generate` accumulator. No
persistence, no shared state. Refresh = new game.

https://github.com/user-attachments/assets/d2e9d1a1-4df6-46c1-b27b-33db7dda132e

## Run

```bash
http-nu --datastar :3002 examples/2048/serve.nu
```

Visit http://localhost:3002 and play.

## What this demonstrates

- **Fan-in via topic glob**: the SSE consumer subscribes to `game.<tabid>.*`,
  so future per-tab impulse types (e.g. `game.<tabid>.undo`) can be added
  without changing the consumer's subscribe call.
- **Per-tab isolation via topic naming**: open the page in two tabs and they
  play independent games -- `data-signals="{tabId: crypto.randomUUID(), ...}"`
  generates a fresh id on each page load, so topics never cross.
