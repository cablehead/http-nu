# SSE pipeline stages for the 2048 server. Composed in serve.nu as:
#
#   .cat --follow --pulse 450
#   | pulse-keepalive
#   | frames-to-states
#   | threshold-gate-states
#   | states-to-html
#   | html-to-patches
#   | to sse
#
# Each stage has one job. Pre-converted SSE event records (from
# pulse-keepalive) pass through every later stage untouched.

use http-nu/datastar *
use ./render.nu *

# Top of pipeline. xs.pulse frames become ready-to-send patch-signals
# events immediately, so downstream stages never have to know about
# pulses -- they just pass anything with an `event` field through.
export def pulse-keepalive [] {
  each {|f|
    if ($f.topic? | default "") == "xs.pulse" {
      ({} | to datastar-patch-signals)
    } else { $f }
  }
}

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
export def threshold-gate-states [] {
  generate {|item state = {}|
    if ('event' in $item) {
      return {out: $item, next: $state}
    }
    if ($item.threshold? | default false) {
      let emit = $state.last? | default ($item | upsert threshold false)
      return {out: $emit, next: {reached: true}}
    }
    if ("reached" in $state) {
      return {out: $item, next: $state}
    }
    {next: ($state | upsert last $item)}
  }
}

# Each state expands into a list of three small renders (board + score
# + state-badge). The board patch uses view-transition (so tiles slide);
# the bar fragments are tagged {vt: false} so they morph in place without
# kicking off their own VT (multiple VTs per state interrupt the tile
# slide animation).
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
      let board = ($state | render-game ($s.direction? | default "") $changed ($s.req_id? | default ""))
      # Only state-changing renders ride a view-transition. Echo patches
      # for no-op moves morph in place (data-rev attribute flip, identical
      # children) -- no pseudos, no re-pop of merged/spawned animations.
      # render-game already includes the state-badge as a board overlay,
      # so it patches alongside the board on every state change.
      [
        {vt: $changed, el: $board}
        {vt: false, el: (render-score $state.score)}
      ]
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
