# xs actor: 2048 game-state singleton.
#
# Stateless per-game. For each impulse, reads `.last game.<id>.snapshot`
# (current HEAD), computes the new state, and appends a new snapshot
# carrying `meta.prev = <previous HEAD's frame id>`. ttl: forever, so the
# tree is preserved.
#
# Undo walks: read HEAD, fetch `head.meta.prev` via `.get`, write a new
# snapshot whose state is the parent's and whose `prev` is the parent's
# `prev` (so successive undos walk back through the chain).
#
# Registered at serve.nu startup. Re-registering replaces the running
# actor, so restarts are safe. Requires `--services`.

{
  run: {|frame, state = null|
    use game *

    let topic = $frame.topic

    # --- New game: player.<uuid>.games frame ----------------------------
    if (($topic | str starts-with "player.") and ($topic | str ends-with ".games")) {
      let game_id = $frame.id
      let player_id = $topic | str replace "player." "" | str replace ".games" ""
      let init = initial-state $game_id
      let max_tile = if ($init.tiles | is-empty) { 0 } else { $init.tiles | get value | math max }
      let moves = [0 ($init.next_id - 3)] | math max
      let req_id = $frame | get meta? | default {} | get req_id? | default ""
      # Root snapshot. prev points at the games_topic frame itself so the
      # chain terminates cleanly. req_id carries the originating reset
      # POST's id so the client's RTT probe resolves.
      null | .append $"game.($game_id).snapshot" --meta {
        state: $init
        last_move_id: $game_id
        prev: $game_id
        intent: "init"
        player_id: $player_id
        req_id: $req_id
        score: 0
        max_tile: $max_tile
        moves: $moves
        game_over: false
      }
      return {next: $state}
    }

    # --- Move impulses: game.<id>.move ---------------------------------
    let is_move_topic = ($topic | str starts-with "game.") and ($topic | str ends-with ".move")
    if not $is_move_topic { return {next: $state} }

    let game_id = $topic | str substring 5.. | str replace ".move" ""
    let kind = $frame.meta | get kind? | default "move"

    # Read current HEAD. Used both as the state to act on (moves) and as
    # the chain pointer (every new snapshot's meta.prev = head.id).
    let head = .last $"game.($game_id).snapshot"
    if $head == null { return {next: $state} }
    let head_state = $head.meta.state
    let player_id = $head.meta | get player_id? | default ""

    # Authz: the move's stamped user_id must match the game's owner.
    # Anonymous-or-mismatched frames are silently dropped here (no
    # snapshot, no state change). HTTP-layer enforcement is the first
    # line; this is the second-line gate -- frames from any source
    # (CLI append, replay, third party) get the same treatment.
    # See game.nu `move-authorized` for the pure rule + tests.
    if not (move-authorized $frame $player_id) {
      return {next: $state}
    }

    let intent = $frame.meta | get intent? | default ""
    let req_id = $frame.meta | get req_id? | default ""

    # Compute (new_state, snap_prev). For moves: act on HEAD's state,
    # link prev to head.id. For undo: walk back via head.meta.prev to
    # the parent snapshot, use its state and inherit its prev so the
    # chain stays consistent for successive undos.
    let result = if $kind == "undo" {
      let parent_id = $head.meta | get prev? | default $game_id
      if $parent_id == $game_id {
        null
      } else {
        let parent = try { .get $parent_id } catch { null }
        if $parent == null { null } else {
          # Clear the parent state's animation annotations before
          # using it as the undo target. `ghosts`, `spawned`, and
          # `merged` were all set by the move that originally
          # produced the parent state -- if we ship them as-is, the
          # WC re-fires that move's animations on every undo (the
          # most visible symptom: merge-pops bouncing on undo).
          let cleaned_tiles = $parent.meta.state.tiles | each {|t|
            $t | upsert spawned false | upsert merged false
          }
          let cleaned_state = $parent.meta.state
            | upsert tiles $cleaned_tiles
            | upsert ghosts []
          {
            state: $cleaned_state
            snap_prev: ($parent.meta | get prev? | default $game_id)
            intent: "undo"
          }
        }
      }
    } else if $intent in [h j k l] {
      let next = $head_state | apply-move $intent $game_id
      if (tiles-equal $head_state.tiles $next.tiles) {
        null
      } else {
        {state: $next, snap_prev: $head.id, intent: $intent}
      }
    } else {
      null
    }

    if $result == null { return {next: $state} }

    let max_tile = if ($result.state.tiles | is-empty) { 0 } else { $result.state.tiles | get value | math max }
    let moves = [0 ($result.state.next_id - 3)] | math max
    null | .append $"game.($game_id).snapshot" --meta {
      state: $result.state
      last_move_id: $frame.id
      prev: $result.snap_prev
      intent: $result.intent
      player_id: $player_id
      req_id: $req_id
      score: $result.state.score
      max_tile: $max_tile
      moves: $moves
      game_over: $result.state.game_over
    }
    {next: $state}
  }
  initial: null
  # Resume cursor: start just after the move frame that produced the most
  # recent snapshot, so a re-register (e.g. --watch reload, restart) picks
  # up any moves that landed in the handover window instead of dropping
  # them. `start: "new"` would tail-start and silently lose those moves --
  # and since the SSE echoes the move frame's req_id independently, the
  # client's pending indicator would clear with no warning. The cursor is
  # the snapshot's `last_move_id` (the frame it applied), NOT the snapshot
  # frame id -- using the snapshot id would skip a move that arrived
  # between a move and its snapshot. Singleton over all games, so it's the
  # globally most-recent snapshot. Reprocessing the trailing no-op / unauth
  # / dead-undo moves after that point is safe -- they append nothing.
  # Empty store -> null -> framework default (new), which is fine: no moves
  # to miss yet.
  start: (.cat | where topic =~ '\.snapshot$' | last 1 | get 0?.meta?.last_move_id?)
}
