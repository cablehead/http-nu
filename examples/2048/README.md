# 2048, event-sourced over xs

A solo-tab 2048 demo where the game state is derived from an append-only event
log stored in [embedded cross.stream](../../README.md#embedded-crossstream-full-featured-persistent-event-stream).
Every move is a frame; replaying the log reconstructs the current board.

- `GET /` mints a fresh tab id and appends a `start` frame seeded with the
  initial board (including the random tile placements, so replay is
  deterministic).
- Each keypress (`hjkl` / arrow keys / `r`) becomes a fire-and-forget POST that
  appends a move frame to `game.<tabid>.move`. The frame meta carries
  `{intent, spawn_idx, spawn_value}` -- the spawn randomness is captured once,
  at POST time, so any number of replays produce the exact same state.
- The SSE handler does `.cat --follow`, replays the per-tab log into a
  `generate` accumulator, gates output on `xs.threshold` so only the
  fully-replayed state ships to the client, then emits a patch per move
  thereafter.

Because state lives in the store, dropped patches and SSE reconnects can't
lose moves -- the POST persists the intent before SSE ever sees it. On
reconnect the SSE handler replays the full log and the client catches up.

https://github.com/user-attachments/assets/d2e9d1a1-4df6-46c1-b27b-33db7dda132e

## Run

The example requires `--store` because it uses `.append` and `.cat`:

```bash
http-nu --datastar --store ./store :3002 examples/2048/serve.nu
```

Visit http://localhost:3002 and play.

## What this demonstrates

- **Event-sourced state.** No server-side mutable variable holds the game.
  The SSE handler is a pure function: log -> state -> patches.
- **Deterministic replay.** Move frames record their spawn seeds so replaying
  the same log always lands on the same board, regardless of when or where
  it replays.
- **`xs.threshold` gating.** During replay the generator silently builds up
  state; only after threshold (history caught up to live) does it ship the
  initial render plus subsequent per-move patches.
- **Persistence across restarts.** Reset the server, refresh the page (with
  the same tab id in a cookie, say) and you'd recover the same game. Keeping
  the log around lets us later build a "see previous games" view.

## Compared to the bus version

The companion example [`../2048-animation/`](../2048-animation/) uses
[`.bus pub` / `.bus sub`](../../README.md#local-bus) instead. That version
needs no `--store` and is simpler to run, but events that arrive during a
brief SSE drop are lost (the bus is broadcast, not a queue). The xs version
trades that for durability.
