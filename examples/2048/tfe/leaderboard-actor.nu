# xs actor: 2048 leaderboard maintainer.
#
# Watches every `game.snapshot.<id>` frame and maintains a single
# `leaderboard.top` head whose meta carries the current top-5, sorted
# by score. Clean runs only: a game that uses any undo is removed from
# the board (even if it ranked earlier), so it ranks fully-unaided runs.
# Per-player dedup: each player gets one slot (their best game), so the
# table stays varied. In-flight games count -- ranking
# uses the snapshot's score, which only climbs (undo can lower a
# player's current game score but never their persisted leaderboard
# slot, because we skip updates whose score is below the player's
# existing slot).
#
# State: in-memory list of the current top-5 entries. First boot on
# an empty `leaderboard.top` topic falls back to `start: "first"` and
# replays every snapshot. Every subsequent boot reads the cursor out
# of the latest `leaderboard.top` frame's meta (`last_processed_id`)
# and resumes after it -- O(new-snapshots-since-last-publish) rather
# than O(all-history). On re-registration (code reload) within the
# same boot, the actor framework resets `$state` to `initial` (null);
# the run lazy-loads the live table from the persisted head so the
# in-memory map stays warm without rewalking the stream.
#
# Output: a `leaderboard.top` frame per change, `ttl last:5`. Readers
# do `.last leaderboard.top | default {} | get meta?.entries? | default []`
# (the `default {}` guards the empty-topic case -- see CLAUDE.md). The
# 5-frame history is a side benefit; the head is the authoritative state.
# `meta` also
# carries `last_processed_id` = the snapshot frame id that produced
# this publish, which is what the cursor-style start expression above
# reads on the next spawn.
#
# Registered at serve.nu startup. Requires --services + --store.

const SIZE = 5

{
  run: {|frame, state = null|
    let topic = $frame.topic

    # Only snapshots matter. Skip everything else cheaply.
    if not ($topic | str starts-with "game.snapshot.") { return {next: $state} }

    # Lazy-load from the persisted head. After a re-registration the
    # framework restarts the closure with $state = initial (null) even
    # though the cursor (last processed frame id) is preserved.
    # Without this, the in-memory table would start empty and only add
    # post-restart scores until 5 climbed in -- the existing leaders
    # would silently vanish from the head.
    let top = if $state == null {
      let head = .last leaderboard.top
      if $head == null { [] } else { $head.meta | get entries? | default [] }
    } else { $state }

    let game_id = $topic | str substring 14..

    # Disqualify on undo: a game that has used any undo is removed from
    # the table -- even if it ranked earlier on a clean snapshot. The
    # count is cumulative and monotonic, so once tainted a game never
    # re-ranks. (Old snapshots lacking the field read as 0 = clean; a
    # rebuild repopulates them.)
    if (($frame.meta | get undos? | default 0) > 0) {
      let pruned = $top | where {|r| $r.game_id != $game_id}
      if ($pruned | length) == ($top | length) { return {next: $top} }
      null | .append "leaderboard.top" --ttl last:5 --meta {
        entries: $pruned
        last_processed_id: $frame.id
      }
      return {next: $pruned}
    }

    let player_id = $frame.meta | get player_id? | default ""
    let score = $frame.meta | get score? | default 0
    # No player attribution or zero score = not a leaderboard candidate.
    if ($player_id | is-empty) or ($score <= 0) { return {next: $top} }

    # If this player already holds an equal-or-higher score in the
    # table, the current snapshot can't improve their slot.
    let existing = $top | where {|r| $r.player_id == $player_id} | get score? | first
    if $existing != null and $existing >= $score { return {next: $top} }

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
    # leaderboard states; readers always use `.last`. `last_processed_id`
    # is the cursor the next spawn resumes after (see start: expression
    # below).
    null | .append "leaderboard.top" --ttl last:5 --meta {
      entries: $candidate
      last_processed_id: $frame.id
    }
    {next: $candidate}
  }
  initial: null
  # Resume from the snapshot id the previous spawn last published a
  # summary for. `| default {}` guards the empty-topic case (fresh /
  # cursor-missing store): without it, .last's empty pipeline crashes
  # `get` ("Pipeline empty" -- see CLAUDE.md); with it, the missing
  # `last_processed_id` reads as null and `default "first"` replays
  # everything once. Subsequent boots become O(new-snapshots).
  start: (.last leaderboard.top | default {} | get meta?.last_processed_id? | default "first")
}
