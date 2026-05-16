# xs actor: 2048 game-state singleton.
#
# Watches player.<uuid>.games (new-game frames) and game.<id>.move
# (impulses). Applies them via game.nu's pure logic; writes the
# resulting state to game.<id>.snapshot (ttl: forever, so the chronology
# of states is preserved as a history).
#
# Each snapshot's meta.last_move_id is the impulse that produced it;
# meta.intent is the move (or "undo" / "init"). Undo writes a snapshot
# whose state is the popped one -- just another chronological event.
#
# Registered at serve.nu startup. Re-registering replaces the running
# actor, so restarts are safe. Requires `--services` on the http-nu (or
# xs serve) process for the actor to actually run.
#
# Single writer -- the SSE handler is a pure reader (no in-pipeline tap).
# Multiple SSE connections (incl. viewers, when added) all read what the
# actor writes.

{
  run: {|frame, state = {games: {}}|
    use game *

    let topic = $frame.topic

    # --- New game: player.<uuid>.games frame ----------------------------
    if (($topic | str starts-with "player.") and ($topic | str ends-with ".games")) {
      let game_id = $frame.id
      let player_id = $topic | str replace "player." "" | str replace ".games" ""
      let init = (initial-state $game_id)
      let max_tile = if ($init.tiles | is-empty) { 0 } else { $init.tiles | get value | math max }
      let moves = [0 ($init.next_id - 3)] | math max
      # Root snapshot of this game.
      null | .append $"game.($game_id).snapshot" --meta {
        state: $init
        last_move_id: $game_id
        intent: "init"
        player_id: $player_id
        score: 0
        max_tile: $max_tile
        moves: $moves
        game_over: false
      }
      return {next: ($state | upsert games ($state.games | upsert $game_id {stack: [$init], player_id: $player_id}))}
    }

    # --- Move impulses: game.<id>.move ---------------------------------
    let is_move_topic = (($topic | str starts-with "game.") and ($topic | str ends-with ".move"))
    if not $is_move_topic { return {next: $state} }

    let game_id = $topic | str substring 5.. | str replace ".move" ""
    let kind = $frame.meta | get kind? | default "move"

    # View-toggle is per-connection ephemeral; not a game-state event.
    if $kind == "view" { return {next: $state} }

    # Lazy-load per-game accumulator on first encounter after restart.
    # Prefer the existing snapshot (cheap); fall back to a full replay.
    let acc = if $game_id in $state.games {
      $state.games | get $game_id
    } else {
      let snap = (try { .last $"game.($game_id).snapshot" } catch { null })
      let loaded = if $snap != null {
        $snap.meta.state
      } else {
        replay-game-state $game_id
      }
      let player_id = if $snap != null {
        $snap.meta | get player_id? | default ""
      } else {
        let owner = (try { .get $game_id } catch { null })
        if $owner != null {
          $owner.topic | str replace "player." "" | str replace ".games" ""
        } else { "" }
      }
      {stack: [$loaded], player_id: $player_id}
    }

    let cur = $acc.stack | last
    let intent = $frame.meta | get intent? | default ""

    let new_acc = if $kind == "undo" {
      if ($acc.stack | length) <= 1 {
        $acc
      } else {
        $acc | update stack ($acc.stack | drop 1)
      }
    } else if $intent in [h j k l] {
      let next = $cur | apply-move $intent $game_id
      if (tiles-equal $cur.tiles $next.tiles) {
        $acc
      } else {
        $acc | update stack ($acc.stack | append $next)
      }
    } else {
      # Empty intent / unknown / legacy slam-X -- noop echo, no snapshot.
      $acc
    }

    let new_top = $new_acc.stack | last
    if (tiles-equal $cur.tiles $new_top.tiles) {
      return {next: ($state | upsert games ($state.games | upsert $game_id $new_acc))}
    }

    # State changed -- write the snapshot. ttl: forever preserves the
    # whole history; meta.last_move_id links each snapshot back to its
    # causing impulse so consumers can walk the chronology.
    let max_tile = if ($new_top.tiles | is-empty) { 0 } else { $new_top.tiles | get value | math max }
    let moves = [0 ($new_top.next_id - 3)] | math max
    let snap_intent = if $kind == "undo" { "undo" } else { $intent }
    null | .append $"game.($game_id).snapshot" --meta {
      state: $new_top
      last_move_id: $frame.id
      intent: $snap_intent
      player_id: $acc.player_id
      score: $new_top.score
      max_tile: $max_tile
      moves: $moves
      game_over: $new_top.game_over
    }
    {next: ($state | upsert games ($state.games | upsert $game_id $new_acc))}
  }
  initial: {games: {}}
  start: "new"
}
