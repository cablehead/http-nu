# 2048 game logic + replay helpers, designed for interactive use from a
# standard nu shell against a vanilla `xs serve` store.
#
#   $env.XS_ADDR = (realpath ./store)
#   overlay use -r examples/2048
#   list-games
#   replay-game-state "<id>"
#   follow-game "<id>" | each { reject state.tiles }   # live tail
#
# Pure game logic (no store deps) lives in game.nu and is re-exported here
# so callers see one surface. The store-dependent helpers below add
# `.cat` / `.last` / `.append` -- the xs snapshot-actor uses game.nu
# directly to avoid module-parse failures.
#
# serve.nu does `use mod.nu *` for everything; this module is the primary
# surface and works without the server.

export use ./game.nu *

# Snapshot writing lives in the xs snapshot-actor (snapshot-actor.nu),
# registered at serve.nu startup. The actor is the single writer to
# `game.<id>.snapshot`; the SSE handler is a pure reader.

# Read-side fold for the SSE pipeline. Takes raw frames from
# `game.<id>.*` (plus xs.threshold / xs.pulse) and yields the
# `{state, threshold?, ...}` record shape that the renderer
# expects, so the rest of the pipeline (threshold-gate-states,
# states-to-html, html-to-patches) is unchanged.
#
# Snapshot frames -> state record.
# xs.threshold / xs.pulse -> pass-through markers.
# Everything else -> drop.
export def frames-to-states [] {
  generate {|f, acc = {state: null}|
    # Pre-converted SSE event records (e.g. from `pulse-keepalive`) just
    # flow through -- they're already shaped for `to sse`.
    if ('event' in $f) {
      return {out: $f, next: $acc}
    }
    let t = $f.topic
    if $t == "xs.threshold" {
      {out: {state: $acc.state, threshold: true}, next: $acc}
    } else if ($t | str ends-with ".snapshot") {
      let state = $f.meta.state
      let intent = $f.meta | get intent? | default ""
      let req_id = $f.meta | get req_id? | default ""
      {
        out: {state: $state, direction: $intent, changed: true, threshold: false, req_id: $req_id, move_id: ($f.meta | get last_move_id? | default "")}
        next: ($acc | upsert state $state)
      }
    } else if ($t | str ends-with ".move") {
      # Echo the current state with the originating req_id. This is the
      # client's RTT-resolution signal: any move POST (RTT ping, real
      # h/j/k/l, undo) gets a quick state-echo patch back that flips
      # #game's data-rev to the matching req_id, so script.js can mark
      # the pending probe as resolved. For *state-changing* moves the
      # actor also writes a snapshot frame; that's emitted separately
      # downstream and is what actually re-renders new tiles. For NO-OP
      # moves (h/j/k/l that don't move any tiles) only the echo lands,
      # so without this branch the client's RTT counter ticks forever.
      let req_id = $f.meta | get req_id? | default ""
      {out: {state: $acc.state, req_id: $req_id, threshold: false}, next: $acc}
    } else {
      {next: $acc}
    }
  }
}

# Replay a game's move log into its final state. Used by serve.nu's /games
# view to render each past game's resting board, and by ad-hoc poking from
# `http-nu eval`.
export def replay-game-state [game_id: string]: nothing -> record {
  (.cat -T $"game.($game_id).move") | project-game $game_id
}

# Resume helper: returns the cheapest known starting point for `game_id`.
#
#   {
#     state          : the current game state (record)
#     follow_from_id : frame id to pass to `.cat --after` to receive only
#                      moves not yet incorporated into `state`
#     moves          : best-effort move count (from snapshot meta if any,
#                      else derived from the resulting state)
#   }
#
# Snapshot path is O(1). Fallback (no snapshot, full replay) is O(N moves)
# but writes the snapshot back to the store so subsequent calls are O(1) --
# self-healing for games that predate the snapshot machinery or were never
# touched by an SSE connection.
export def resume-game [game_id: string]: nothing -> record {
  let snapshot = (try { .last $"game.($game_id).snapshot" } catch { null })
  if $snapshot != null {
    return {
      state: $snapshot.meta.state
      follow_from_id: $snapshot.meta.last_move_id
      moves: $snapshot.meta.moves
    }
  }
  # No snapshot -- full replay, then backfill so we don't pay this twice.
  let state = (replay-game-state $game_id)
  let moves = [0 ($state.next_id - 3)] | math max
  let last_move = (try { .last $"game.($game_id).move" } catch { null })
  let follow_from_id = if $last_move != null { $last_move.id } else { $game_id }
  let max_tile = if ($state.tiles | is-empty) { 0 } else { $state.tiles | get value | math max }
  # game_id is the id of the player.<uuid>.games frame that started this
  # game; fetch it to recover the owning player_id for the snapshot meta.
  let owner_frame = (try { .get $game_id } catch { null })
  let player_id = if $owner_frame != null {
    $owner_frame.topic | str replace "player." "" | str replace ".games" ""
  } else { "" }
  null | .append $"game.($game_id).snapshot" --meta {
    state: $state
    last_move_id: $follow_from_id
    intent: "backfill"
    player_id: $player_id
    score: $state.score
    max_tile: $max_tile
    moves: $moves
    game_over: $state.game_over
  }
  {state: $state, follow_from_id: $follow_from_id, moves: $moves}
}

# Live-follow a game: replays the existing move log, then yields one record
# per state change as new moves are appended. Output is `{state, mode?,
# direction?, changed?, threshold?, req_id?}` -- the same shape
# impulses-to-states emits, with non-state items (signals, pulses) filtered
# out so the stream is one state-per-tick. Streams indefinitely.
#
#   http-nu eval --store ./store -c '
#     use examples/2048/mod.nu *
#     follow-game "<game_id>" | each { reject state.tiles }
#   '
export def follow-game [game_id: string] {
  .cat --follow -T $"game.($game_id).move"
  | impulses-to-states {
    stack: [(initial-state $game_id)]
    game_id: $game_id
    games_topic: ""
  }
  | where ($it | get state? | default null) != null
}

# List every game in the store with its move count, biggest first.
export def list-games [] {
  .cat
  | where ($it.topic | str starts-with "game.") and ($it.topic | str ends-with ".move")
  | group-by topic
  | items {|topic frames|
      {
        game: ($topic | str replace "game." "" | str replace ".move" "")
        moves: ($frames | length)
      }
    }
  | sort-by moves --reverse
}

# List every player seen in the store with their game count and latest game id.
export def list-players [] {
  .cat
  | where ($it.topic | str starts-with "player.") and ($it.topic | str ends-with ".games")
  | group-by topic
  | items {|topic frames|
      {
        player: ($topic | str replace "player." "" | str replace ".games" "")
        games: ($frames | length)
        latest_game: ($frames | last | get id)
      }
    }
}
