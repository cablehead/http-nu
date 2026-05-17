# nu2048

2048 with a per-player game library, durable state, and animated SSE
patches. Built on http-nu + cross.stream + Datastar.

https://github.com/user-attachments/assets/d2e9d1a1-4df6-46c1-b27b-33db7dda132e

## Run

Requires `--store` (for `.append` / `.cat`) and `--services` (for the
snapshot-actor):

```bash
http-nu --datastar --services --store ./store :3002 examples/2048/serve.nu
```

http://localhost:3002.

## How state moves

```
POST /move        ->  appends `game.<id>.move`        (intent only)
snapshot-actor    ->  reads .last snapshot, applies move, appends
                       `game.<id>.snapshot`            (canonical state)
GET  /sse/<id>    ->  follows `game.<id>.*`, gates on xs.threshold,
                       renders Datastar element patches
```

Move POSTs never compute state. A singleton xs actor owns writes, so two
tabs of the same game cannot race. The SSE handler is a pure reader of
the snapshot stream.

The `roll` helper hashes `(game_id, state, key)` for tile spawns; no
random seed is stored in frames. Replay reconstructs the same board.

## Layout

```
serve.nu                routes
tfe/
  game.nu               pure logic (slide, spawn, roll, apply-move)
  store.nu              .cat/.last wrappers (resume-game, list-games)
  render.nu             HTML output (board, card, layout)
  sse.nu                SSE pipeline (frames-to-states -> patches)
  snapshot-actor.nu     xs actor source (registered at startup)
  templates/            layout.html, vt-tuner.html
static/                 styles.css, script.js, og.png, ellie.png
test/                   unit tests, browser e2e, benchmark
```

`game.nu` and `store.nu` have no http-nu dependencies; they work from a
plain nu shell against a vanilla `xs serve` store.

## CLI

```nushell
$env.XS_ADDR = (realpath ./store)
overlay use -r examples/2048/tfe

list-players                            # players seen, with game counts
list-games | first 5                    # games by move count
resume-game "03g54..." | reject state.tiles
follow-game "03g54..." | each { reject state.tiles }

# Replay from raw move frames (no snapshots needed):
.cat -T "game.03g54.move" | project-game "03g54" | reject tiles
```

## Animation

Each move runs three sequential phases on one view-transition: slide,
merge pop, spawn-in. CSS targets `view-transition-class` (set in
`render.nu` per tile: `merged`, `spawned`, `ghost`). The `[ fx ]` button
in the help panel opens a tuner for the timing knobs (durations + the
merge-pop scale + the spawn-in starting scale). Defaults live in
`static/styles.css :root`.
