# lib/send.nu -- xs send-on-event handler.
#
# Tails `email.send.requested`, dispatches each through the `email send`
# plugin command, and appends an `email.send.<outcome>` frame per result so
# the lifecycle is fully visible in xs (see docs/email-native/events.md).
#
# Each requested frame's `content` is the JSON-serialized EmailRequest;
# `meta.request_ref` (if any) is mirrored on the outcome for correlation,
# and the originating frame id is recorded as `meta.request_id`.
#
# Run as a long-lived xs handler -- e.g.
#   xs handler --topic email.send.requested ./examples/email-demo/lib/send.nu
# or invoke directly with `nu -c "use ./examples/email-demo/lib/send.nu *; dispatch_loop"`.

# Map `email send`'s `result` field (Outcome::as_str()) to the xs topic.
export def outcome_topic [outcome: string]: nothing -> string {
    match $outcome {
        "delivered" => "email.send.delivered"
        "rate_limited" => "email.send.rate_limited"
        "daily_quota_exceeded" => "email.send.daily_quota_exceeded"
        _ => "email.send.failed"
    }
}

# Convert a `SendResult` record into the xs meta payload, dropping nulls.
export def result_to_meta [result: record, request_id: string]: nothing -> record {
    {
        request_ref: ($result.request_ref? | default null),
        message_id: ($result.message_id? | default null),
        error_code: ($result.error_code? | default null),
        message: ($result.message? | default null),
        retry_after: ($result.retry_after? | default null),
        request_id: $request_id,
    }
}

# Dispatch a single frame. The plugin's `email send` returns a record per
# input; we forward exactly one outcome frame per request.
export def dispatch_one [frame: record] {
    let req = ($frame.content | from json)
    let result = ($req | email send)
    let topic = (outcome_topic $result.result)
    null | .append $topic --meta (result_to_meta $result $frame.id)
}

# Long-lived: follow xs and dispatch every new request_one. Stop with ^C.
#
# Implementation note: `.cat --follow --new --topic email.send.requested`
# is the idiomatic http-nu way to tail a topic. The `--new` flag picks up
# the cursor from the latest frame at startup; restart-safe for the demo.
# Production may want explicit cursor management.
export def dispatch_loop [] {
    .cat --follow --new --topic email.send.requested | each {|frame|
        dispatch_one $frame
    }
}
