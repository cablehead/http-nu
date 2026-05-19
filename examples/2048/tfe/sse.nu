# SSE pipeline stages for the 2048 server. Composed in serve.nu as:
#
#   .cat --follow
#   | frames-to-states
#   | threshold-gate-states
#   | states-to-wc-signals
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
      # Every move frame emits a state record carrying its req_id.
      # Downstream emits a {lastReqId} signal patch the client uses to
      # resolve its pending RTT probe (window.onAck in script.js).
      # State-changing moves also produce a snapshot frame (its own
      # record, emitted separately) -- that's where the new board
      # state actually flows. No-op moves produce no snapshot, so this
      # echo is their only resolution.
      let req_id = $f.meta | get req_id? | default ""
      {out: {state: $acc.state, req_id: $req_id, threshold: false}, next: $acc}
    } else {
      {next: $acc}
    }
  }
}

# Buffers states pre-threshold (only the last is retained); on
# threshold marker emits the last buffered state; then forwards
# everything. Forces `changed: true` on the emitted record so the
# threshold flush always pushes a full board patch -- otherwise a
# game whose last frame was a no-op move would flush as a
# {changed: false} echo (lastReqId only), and the page-load WC would
# never see the current snapshot.
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

# State records -> Datastar signal patches for the <game-board> WC.
#   no state          -> drop (placeholder stays put until snapshot lands)
#   no-op move echo   -> {lastReqId}                  (the ack only)
#   state-changing    -> {boardState, score, gameStatus, lastReqId}
# The WC diffs by tile id and runs slide -> merge-pop -> spawn-in on
# its own, so the wire payload strips animation hints (spawned /
# merged / ghosts) -- pure state.
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
        let board = $state | state-for-wc
        # gameStatus is still derived server-side for surfaces (chrome
        # outside the WC) that want it -- the WC itself derives the
        # badge from boardState.gameOver + tile values.
        let won = $state.tiles | any {|t| $t.value >= 2048 }
        let status = if $won { "won" } else if $state.game_over { "over" } else { "" }
        [{signals: {boardState: $board, score: $state.score, gameStatus: $status, lastReqId: $req_id}}]
      } else {
        [{signals: {lastReqId: $req_id}}]
      }
    }
  }
  | flatten
}

# Convert pipeline records into SSE events. Pre-converted events
# (replayMs etc.) pass through; signal records become signal patches.
# The pipeline no longer produces element/HTML patches now that every
# live board renders as a <game-board>.
export def html-to-patches [] {
  each {|item|
    if ('event' in $item) {
      $item
    } else {
      $item.signals | to datastar-patch-signals
    }
  }
}
