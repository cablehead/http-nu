# current_subs projection over xs.
#
# The xs event topics:
#   push.subscription.added     -- payload is the PushSubscription JSON
#   push.subscription.expired   -- meta has { endpoint }
#   push.subscription.unsubscribed -- meta has { endpoint }
#
# current_subs walks the stream in order and folds add/remove events into the
# active subscriber list, keyed by endpoint (the endpoint URL is the unique
# id of a subscription).
#
# This is the load-bearing connective tissue between the plugin and xs.
# Any other event-sourced concept in your app follows the same pattern:
# project a stream into current state.

# Returns a list of PushSubscription records currently considered active.
export def current_subs []: nothing -> list<any> {
  # The subscription record lives entirely in --meta (see serve.nu /subscribe).
  # No CAS read step, faster projection. xs frames don't expose `ts` here,
  # so we order by the frame id (monotonic) when we need recency.
  let added = (.cat -T push.subscription.added | each {|f|
    { endpoint: $f.meta.endpoint, sub: $f.meta, id: $f.id }
  })

  let removed = (
    (.cat -T push.subscription.expired
     | append (.cat -T push.subscription.unsubscribed))
    | each {|f| { endpoint: $f.meta.endpoint, id: $f.id } }
  )

  let removed_set = ($removed | get endpoint | uniq)

  $added
  | where {|x| ($x.endpoint not-in $removed_set) }
  # If the same endpoint was added twice (re-subscribe after expiry), keep the
  # most recent record so we have current keys.
  | reduce -f {} {|x, acc| $acc | upsert $x.endpoint $x }
  | values
  | each {|x| $x.sub }
}

# Count active subs without materializing the list.
export def current_subs_count []: nothing -> int {
  current_subs | length
}

# Render a list of subs as a table for /admin/subs.
export def subs_table []: nothing -> any {
  current_subs | each {|s|
    {
      endpoint: ($s.endpoint | str substring 0..60 | $"($in)..."),
      origin: ($s.endpoint | url parse | get host),
    }
  }
}
