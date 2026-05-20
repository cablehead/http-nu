# xs actor: 2048 leaderboard maintainer.
#
# Watches every `game.<id>.snapshot` frame and maintains a single
# `leaderboard.top` head whose meta carries the current top-5, sorted
# by score. Per-player dedup: each player gets one slot (their best
# game), so the table stays varied. In-flight games count -- ranking
# uses the snapshot's score, which only climbs (undo can lower a
# player's current game score but never their persisted leaderboard
# slot, because we skip updates whose score is below the player's
# existing slot).
#
# State: in-memory list of the current top-5 entries. On first run
# starts empty and rebuilds as `start: "first"` replays the full
# history of snapshots. On re-registration (code reload), the actor
# framework resets `$state` to `initial` (null) even though it
# preserves the frame cursor -- so the run lazy-loads from the
# persisted head, preserving the table without re-replaying history.
#
# Output: a `leaderboard.top` frame per change, `ttl last:5`. Readers
# do `.last leaderboard.top | get meta.entries`. The 5-frame history
# is a side benefit -- handy for inspecting recent churn -- but the
# head is the authoritative state.
#
# Registered at serve.nu startup. Requires --services + --store.

const SIZE = 5

{
  run: {|frame, state = null|
    let topic = $frame.topic

    # Only snapshots matter. Skip everything else cheaply.
    let is_snapshot = ($topic | str starts-with "game.") and ($topic | str ends-with ".snapshot")
    if not $is_snapshot { return {next: $state} }

    # Lazy-load from the persisted head. After a re-registration the
    # framework restarts the closure with $state = initial (null) even
    # though the cursor (last processed frame id) is preserved.
    # Without this, the in-memory table would start empty and only add
    # post-restart scores until 5 climbed in -- the existing leaders
    # would silently vanish from the head.
    let top = if $state == null {
      let head = try { .last leaderboard.top } catch { null }
      if $head == null { [] } else { $head.meta | get entries? | default [] }
    } else { $state }

    let player_id = $frame.meta | get player_id? | default ""
    let score = $frame.meta | get score? | default 0
    # No player attribution or zero score = not a leaderboard candidate.
    if ($player_id | is-empty) or ($score <= 0) { return {next: $top} }

    # If this player already holds an equal-or-higher score in the
    # table, the current snapshot can't improve their slot.
    let existing = $top | where {|r| $r.player_id == $player_id} | get score? | first
    if $existing != null and $existing >= $score { return {next: $top} }

    let game_id = $topic | str substring 5.. | str replace ".snapshot" ""
    let entry = {
      player_id: $player_id
      game_id: $game_id
      score: $score
      max_tile: ($frame.meta | get max_tile? | default 0)
      moves: ($frame.meta | get moves? | default 0)
      game_over: ($frame.meta | get game_over? | default false)
    }

    # Build new top-N: drop player's old entry (if any), append the
    # new one, sort by score desc, truncate.
    let candidate = $top
      | where {|r| $r.player_id != $player_id}
      | append $entry
      | sort-by score -r
      | first $SIZE

    # Append the new head. ttl last:5 keeps a small rolling history of
    # leaderboard states; readers always use `.last`.
    null | .append "leaderboard.top" --ttl last:5 --meta {entries: $candidate}
    {next: $candidate}
  }
  initial: null
  start: "first"
}
