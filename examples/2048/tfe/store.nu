# Store-touching helpers for the 2048 example, designed for interactive
# use from a standard nu shell against a vanilla `xs serve` store:
#
#   $env.XS_ADDR = (realpath ./store)
#   use examples/2048/tfe *
#   list-games
#   replay-game-state "<id>"
#   follow-game "<id>" | each { reject state.tiles }   # live tail
#
# Pure game logic (no store deps) lives in game.nu and is consumed by
# this module via `use ./game.nu *`. The xs snapshot-actor uses game.nu
# directly to avoid module-parse failures.

use ./game.nu *

# Snapshot writing lives in the xs snapshot-actor (snapshot-actor.nu),
# registered at serve.nu startup. The actor is the single writer to
# `game.<id>.snapshot`; readers (SSE + helpers below) treat it as
# authoritative HEAD.

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

# Top games by score within `--since` (default 7day), limited to `--limit`
# (default 5). Each row carries player, moves, score, max tile, undo count,
# and the game's start time (decoded from its SCRU128 frame id).
export def leaderboard [--since: duration = 7day, --limit: int = 5] {
  let cutoff = (date now) - $since
  .cat
  | where ($it.topic | str starts-with "game.") and (($it.topic | str ends-with ".move") or ($it.topic | str ends-with ".snapshot"))
  | insert game {|f| $f.topic | split row "." | get 1 }
  | insert kind {|f| $f.topic | split row "." | last }
  | group-by game
  | items {|game frames|
      let moves = $frames | where kind == "move"
      let head = $frames | where kind == "snapshot" | last
      {
        game: ($game | str substring 0..7)
        when: ($game | .id unpack | get timestamp)
        player: ($head.meta.player_id? | default "" | str substring 0..7)
        moves: ($moves | where ($it.meta?.kind? | default "") != "undo" | length)
        score: ($head.meta.score? | default 0)
        max: ($head.meta.max_tile? | default 0)
        undos: ($moves | where ($it.meta?.kind? | default "") == "undo" | length)
      }
    }
  | where when >= $cutoff
  | sort-by score -r
  | first $limit
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
