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
# `game.snapshot.<id>`; readers (SSE + helpers below) treat it as
# authoritative HEAD.

# Replay a game's move log into its final state. Used by serve.nu's /games
# view to render each past game's resting board, and by ad-hoc poking from
# `http-nu eval`.
export def replay-game-state [game_id: string]: nothing -> record {
  (.cat -T $"game.move.($game_id)") | project-game $game_id
}

# Read-only HEAD lookup: returns the snapshot-actor's current head for
# `game_id` as
#
#   {
#     state          : the current game state (record)
#     follow_from_id : frame id to pass to `.cat --after` to receive only
#                      moves not yet incorporated into `state`
#     moves          : best-effort move count (snapshot meta, else 0)
#   }
#
# The snapshot-actor is the sole writer of `game.snapshot.<id>`; it
# rebuilds every game's chain from the move log on boot (`start: "first"`
# when the store has no snapshots -- see snapshot-actor.nu), so readers
# never backfill. A game with no snapshot yet (actor still catching up,
# or just created) reads as its deterministic initial board; the actor
# fills in the real head shortly and /sse-wc patches it live.
export def game-head [game_id: string]: nothing -> record {
  let snapshot = .last $"game.snapshot.($game_id)"
  if $snapshot != null {
    return {
      state: $snapshot.meta.state
      follow_from_id: $snapshot.meta.last_move_id
      moves: $snapshot.meta.moves
    }
  }
  {state: (initial-state $game_id), follow_from_id: $game_id, moves: 0}
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
  .cat --follow -T $"game.move.($game_id)"
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
  | where ($it.topic | str starts-with "game.move.")
  | group-by topic
  | items {|topic frames|
      {
        game: ($topic | str substring 10..)
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
  # Walk players -> their games (indexed by topic), pulling each game's
  # HEAD snapshot (indexed by topic). The snapshot's id timestamp is the
  # last activity -- gate on that before keeping the row, so games that
  # went quiet earlier than `--since` cost only one `.last` each.
  # Move counts (for the undo column) are scanned only for the top N.
  list-players | each {|p|
    .cat -T $"player.($p.player).games" | each {|f|
      let snap = .last $"game.snapshot.($f.id)"
      if $snap == null { return null }
      let when = $snap.id | .id unpack | get timestamp
      if $when < $cutoff { return null }
      {
        game: ($f.id | str substring 0..7)
        game_id: $f.id
        when: $when
        player: ($p.player | str substring 0..7)
        score: ($snap.meta.score? | default 0)
        max: ($snap.meta.max_tile? | default 0)
        moves: ($snap.meta.moves? | default 0)
      }
    }
  }
  | flatten | compact
  | sort-by score -r
  | first $limit
  | insert undos {|row|
      .cat -T $"game.move.($row.game_id)"
      | where ($it.meta?.kind? | default "") == "undo" | length
    }
  | reject game_id
}

# Per-player best across all time, in-flight or not. One row per player
# (their highest-scoring game's HEAD snapshot), sorted by score, top N.
# Used by the /leaderboard page; the per-game/time-windowed CLI helper
# `leaderboard` stays for ad-hoc poking.
#
# Strategy: one `.cat` for the `player.*.games` ledger gives us every
# game-id in the store; one `.last game.snapshot.<id>` per game gives
# us the head row. Group by player, pick best per player, sort, head N.
# See test/bench-leaderboard.nu for the scaling profile.
export def top-players [--limit: int = 10] {
  .cat
  | where ($it.topic | str starts-with "player.") and ($it.topic | str ends-with ".games")
  | each {|f|
      let snap = .last $"game.snapshot.($f.id)"
      if $snap == null { return null }
      let when = try { $snap.id | .id unpack | get timestamp } catch { null }
      {
        game_id: $f.id
        when: $when
        player_id: ($snap.meta | get player_id? | default "")
        player: (($snap.meta | get player_id? | default "") | str substring 0..7)
        score: ($snap.meta | get score? | default 0)
        max_tile: ($snap.meta | get max_tile? | default 0)
        moves: ($snap.meta | get moves? | default 0)
        game_over: ($snap.meta | get game_over? | default false)
      }
    }
  | compact
  | group-by player_id
  | values
  | each {|games| $games | sort-by score -r | first }
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
