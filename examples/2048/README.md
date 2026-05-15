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

## Interactive use from nushell

[`mod.nu`](mod.nu) is the primary surface: pure game logic plus replay
helpers, designed to be loaded into a vanilla nu shell talking to a vanilla
`xs serve` store. `serve.nu` is a web view layered on top; you don't need
it running to explore games from the command line.

Point your nu session at the store and overlay-use the module:

```nushell
$env.XS_ADDR = (realpath ./store)
overlay use -r examples/2048
```

(`xs.nu` -- the CLI wrapper that supplies `.cat`, `.last`, `.append` --
needs to be in your shell already; it shells out to the `xs` binary which
reads `XS_ADDR`.)

Then explore:

```nushell
# Every player seen in the store, with game count + latest game id.
list-players

# Every game, ranked by move count.
list-games | first 5

# Final state of one game.
replay-game-state "03g4l0uvw5ewry8bwhzvgmuye" | reject tiles

# Pipeline form: pipe an arbitrary move-frame stream through the same fold.
# The game_id seeds the deterministic spawn rolls.
.cat -T "game.03g4l0uvw5ewry8bwhzvgmuye.move" | project-game "03g4l0uvw5ewry8bwhzvgmuye" | reject tiles

# Live tail: emits one record per state change as moves stream in. Streams
# indefinitely; Ctrl-C to stop.
follow-game "03g4l0uvw5ewry8bwhzvgmuye" | each { reject state.tiles }
```

Both `replay-game-state` and `follow-game` are thin wrappers over the
streaming primitive `project-game`; the underlying state machine lives in
`impulses-to-states`. `mod.nu` also exports the pure pieces
(`initial-state`, `apply-move`, `slide-tiles`, `tiles-equal`,
`is-game-over`, `roll`, `spawn-tile`, `filter-for-player`) for finer-grained
poking.

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
