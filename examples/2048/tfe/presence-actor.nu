# xs actor: site-wide tab presence aggregator.
#
# Each open tab POSTs /presence/ping every PING_INTERVAL_MS with
# {tabId, scope, gameId?}. The handler appends an ephemeral
# `_presence.ping` frame; this actor watches that topic and maintains
# an in-memory map of currently-live tabs. On every xs.pulse it
# prunes tabs that haven't pinged within TTL_MS and republishes the
# aggregate summary if it changed.
#
# Output: `_presence.summary` (ttl last:1) -- a single record carrying
#
#   {totalTabs, activeGames, byScope: {<scope>: <n>}, byGame: {<id>: <n>}}
#
# Readers do `.last _presence.summary` for seed-on-connect or
# `.cat -T _presence.summary --follow` to track changes live.
#
# State shape: {<tabId>: {scope, gameId, user_id, ts_ms}}.
#
# start: "new" because pings are ephemeral -- there's no history to
# backfill, and after a restart the next round of pings (within
# PING_INTERVAL_MS) refills the map. The transient empty-state window
# is acceptable for a presence display.
#
# Registered at serve.nu startup. Requires --services + --store.

const PING_INTERVAL_MS = 3000
const TTL_MS = (3000 * 2 + 1000)  # 2 missed pings + grace

def now-ms []: nothing -> int { (date now | into int) / 1_000_000 | into int }

def summarize [tabs: record]: nothing -> record {
  let entries = $tabs | values
  let live_ids = $entries | get tabId? | default [] | uniq
  let total = $live_ids | length

  let by_scope = $entries
    | group-by {|e| $e.scope | default ""}
    | items {|k v| {scope: $k, n: ($v | length)}}
    | reduce -f {} {|r acc| $acc | upsert $r.scope $r.n}

  let game_entries = $entries | where {|e| ($e | get gameId? | default "") != ""}
  let by_game = $game_entries
    | group-by {|e| $e.gameId}
    | items {|k v| {gameId: $k, n: ($v | length)}}
    | reduce -f {} {|r acc| $acc | upsert $r.gameId $r.n}

  {
    totalTabs: $total
    activeGames: ($by_game | columns | length)
    byScope: $by_scope
    byGame: $by_game
  }
}

{
  run: {|frame, state = null|
    let topic = $frame.topic

    # Lazy-init both halves of the state on first call:
    #   tabs: {tabId -> {scope, gameId, user_id, ts_ms}}
    #   last_summary: the most recently published payload (for diff-skip)
    let st = if $state == null { {tabs: {}, last_summary: null} } else { $state }

    if $topic == "_presence.ping" {
      let m = $frame.meta | default {}
      let tab_id = $m | get tabId? | default ""
      if ($tab_id | is-empty) { return {next: $st} }
      let entry = {
        tabId: $tab_id
        scope: ($m | get scope? | default "")
        gameId: ($m | get gameId? | default "")
        user_id: ($m | get user_id? | default "")
        ts_ms: (now-ms)
      }
      let new_tabs = $st.tabs | upsert $tab_id $entry
      return {next: {tabs: $new_tabs, last_summary: $st.last_summary}}
    }

    if $topic == "xs.pulse" {
      # Prune expired tabs.
      let cutoff = (now-ms) - $TTL_MS
      let kept_pairs = $st.tabs | transpose k v | where {|p| $p.v.ts_ms >= $cutoff}
      let kept = $kept_pairs | reduce -f {} {|p acc| $acc | upsert $p.k $p.v}

      let summary = summarize $kept
      if $summary == $st.last_summary { return {next: {tabs: $kept, last_summary: $st.last_summary}} }
      null | .append "_presence.summary" --ttl last:1 --meta $summary
      return {next: {tabs: $kept, last_summary: $summary}}
    }

    {next: $st}
  }
  initial: null
  start: "new"
  pulse: 2000
}
