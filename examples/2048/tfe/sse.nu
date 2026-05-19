# SSE pipeline stages for the 2048 server. Composed in serve.nu as:
#
#   .cat --follow
#   | frames-to-states
#   | threshold-gate-states
#   | states-to-html       (or states-to-wc-signals for /sse-wc/<id>)
#   | html-to-patches
#   | to sse
#
# Each stage has one job. The client drives heartbeat / liveness: an
# initial ping fires from script.js on the Datastar `started` event
# and the existing /move + lastReqId path resolves its RTT. The server
# no longer emits keepalives, so SSE handlers wake only on real frames.

use http-nu/datastar *
use ./render.nu *

# Frames -> state records.
#   snapshot frames      -> {state, direction, changed, req_id, move_id, threshold: false}
#   move frames          -> {state: <last seen>, req_id, threshold: false}  (RTT echo)
#   xs.threshold marker  -> {state, threshold: true}
#   everything else      -> dropped
# Pre-converted events pass through.
export def frames-to-states [] {
  generate {|f, acc = {state: null}|
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
      # Every move frame emits a state record carrying its req_id. The
      # render flips #game's data-rev to that req_id, which is the
      # client's signal that its pending RTT probe has resolved.
      # State-changing moves also produce a snapshot frame (emitted
      # downstream as its own record), and that one re-renders new
      # tiles. No-op moves produce no snapshot, so this echo is their
      # only resolution.
      let req_id = $f.meta | get req_id? | default ""
      {out: {state: $acc.state, req_id: $req_id, threshold: false}, next: $acc}
    } else {
      {next: $acc}
    }
  }
}

# Buffers states pre-threshold (only the last is retained); on threshold
# marker emits the last buffered state; then forwards everything.
# Forces `changed: true` on the emitted record so the threshold flush
# always paints the board -- otherwise a game whose last frame was a
# no-op move would flush as a {changed: false} echo and states-to-html
# would emit a signals-only patch, leaving the placeholder in place.
export def threshold-gate-states [] {
  generate {|item state = {}|
    if ('event' in $item) {
      return {out: $item, next: $state}
    }
    if ($item.threshold? | default false) {
      let emit = $state.last?
        | default ($item | upsert threshold false)
        | upsert changed true
      return {out: $emit, next: {reached: true}}
    }
    if ("reached" in $state) {
      return {out: $item, next: $state}
    }
    {next: ($state | upsert last $item)}
  }
}

# Each state expands into a list of three small renders (board + score
# + signals). The board patch uses view-transition (so tiles slide);
# score + move-ack ride as a separate signals patch so there's no
# sibling DOM mutation racing with startViewTransition's capture window
# (Safari is sensitive to this; Chrome happens to tolerate it).
export def states-to-html [] {
  each {|s|
    if ('event' in $s) {
      [$s]
    } else if ('signals' in $s) {
      [$s]
    } else if (($s | get -o state) == null) {
      # Fresh game whose snapshot-actor hasn't yet written a snapshot:
      # frames-to-states' acc.state is null when the pulse-threshold
      # arrives. Drop the record -- the /play page's server-rendered
      # placeholder board stays in the DOM until the next real snapshot
      # frame lands. Guarded by test 0a.
      []
    } else {
      let state = $s.state
      let changed = $s.changed? | default false
      let req_id = $s.req_id? | default ""
      # Split by frame kind so each signal has one source:
      #   move frame (changed:false)  -> {lastReqId}     (the ack)
      #   snapshot   (changed:true)   -> {score} + board (the state)
      # The ack lands the moment the SSE pipeline sees the move frame,
      # before the snapshot-actor runs. The snapshot's req_id stays in
      # the appended frame's meta (audit trail) but doesn't ride the
      # wire a second time.
      if $changed {
        # lastReqId rides every state-changing patch too. Threshold-gate
        # absorbs the empty-intent ping into the initial flush; without
        # lastReqId here, the client's ping ack would never arrive and
        # its 1s heartbeat timer would flip conn=down on every /play
        # load. Snapshot frames also carry their originating move's
        # req_id, so this resolves every move's pending in one channel.
        [
          {signals: {score: $state.score, lastReqId: $req_id}}
          {vt: true, el: ($state | render-game)}
        ]
      } else {
        [{signals: {lastReqId: $req_id}}]
      }
    }
  }
  | flatten
}

# WC-friendly variant of states-to-html: emits only signal patches so a
# client running <game-board> can drive its rendering from $boardState.
# Same gating shape as states-to-html (no state -> drop, no-op move ->
# lastReqId ack, changed -> {boardState, score, gameStatus}). Strips
# the snapshot's animation hints (spawned / merged / ghosts) from the
# wire payload -- the WC diffs by tile id and doesn't read them.
export def states-to-wc-signals [] {
  each {|s|
    if ('event' in $s) {
      [$s]
    } else if ('signals' in $s) {
      [$s]
    } else if (($s | get -o state) == null) {
      []
    } else {
      let state = $s.state
      let changed = $s.changed? | default false
      let req_id = $s.req_id? | default ""
      if $changed {
        let won = $state.tiles | any {|t| $t.value >= 2048 }
        let status = if $won { "won" } else if $state.game_over { "over" } else { "" }
        let board = {
          tiles: ($state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
        }
        [{signals: {boardState: $board, score: $state.score, gameStatus: $status, lastReqId: $req_id}}]
      } else {
        [{signals: {lastReqId: $req_id}}]
      }
    }
  }
  | flatten
}

# Wrap each render in a datastar patch event. Unique id per patch so
# morphdom replays each step independently (no event dedup).
export def html-to-patches [] {
  each {|item|
    if ('event' in $item) {
      $item
    } else if ('signals' in $item) {
      $item.signals | to datastar-patch-signals
    } else if (('vt' in $item) and not $item.vt) {
      $item.el | to datastar-patch-elements --id (random uuid)
    } else {
      let el = if ('el' in $item) { $item.el } else { $item }
      $el | to datastar-patch-elements --use-view-transition --id (random uuid)
    }
  }
}
