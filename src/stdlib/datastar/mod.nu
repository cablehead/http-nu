# Datastar SSE SDK for Nushell
#
# Generates Server-Sent Events for the Datastar hypermedia framework.
# Follows https://github.com/starfederation/datastar/blob/develop/sdk/ADR.md

# Set required SSE headers (called automatically by to sse-* commands)
export def init-response []: nothing -> nothing {
  .response {
    headers: {
      "content-type": "text/event-stream"
      "cache-control": "no-cache"
      "connection": "keep-alive"
    }
  }
}

# Format SSE event per RFC 8895
def format-sse-event [
  event_type: string # SSE event type
  data_lines: list<string> # Data lines to send
  --event_id: string # Optional event ID for client tracking
  --retry: int # Optional retry interval in milliseconds
]: nothing -> string {
  mut output = []

  # Event type
  $output = ($output | append $"event: ($event_type)")

  # Event ID (optional)
  if ($event_id | is-not-empty) {
    $output = ($output | append $"id: ($event_id)")
  }

  # Retry interval (optional)
  if ($retry | is-not-empty) {
    $output = ($output | append $"retry: ($retry)")
  }

  # Data lines
  for line in $data_lines {
    $output = ($output | append $"data: ($line)")
  }

  # SSE requires blank line after each event
  $output = ($output | append "")

  $output | str join "\n"
}

# Patch HTML elements via SSE
#
# Elements must be complete, well-formed HTML (not fragments).
# Modes: outer (default), inner, replace, prepend, append, before, after, remove
export def "to sse-patch-elements" [
  --selector: string # CSS selector. If omitted, elements must have IDs
  --mode: string = "outer" # outer, inner, replace, prepend, append, before, after, remove
  --use_view_transition # Enable View Transitions API
  --event_id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: string -> string {
  let elements = $in
  init-response

  mut data_lines = []
  if ($selector != null) { $data_lines = ($data_lines | append $"selector ($selector)") }
  if $mode != "outer" { $data_lines = ($data_lines | append $"mode ($mode)") }
  if $use_view_transition { $data_lines = ($data_lines | append "useViewTransition true") }
  if ($elements | str length) > 0 {
    for line in ($elements | lines) { $data_lines = ($data_lines | append $"elements ($line)") }
  }

  format-sse-event "datastar-patch-elements" $data_lines --event_id $event_id --retry $retry
}

# Patch signals via SSE (JSON Merge Patch RFC 7386)
export def "to sse-patch-signals" [
  --only_if_missing # Only set signals missing on client
  --event_id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: record -> string {
  let signals = $in
  init-response

  mut data_lines = []
  if $only_if_missing { $data_lines = ($data_lines | append "onlyIfMissing true") }
  let json_str = $signals | to json --raw
  for line in ($json_str | lines) { $data_lines = ($data_lines | append $"signals ($line)") }

  format-sse-event "datastar-patch-signals" $data_lines --event_id $event_id --retry $retry
}

# Execute JavaScript via SSE (appends <script> to body)
export def "to sse-execute-script" [
  --auto_remove = true # Remove script after execution
  --attributes: record # HTML attributes for script tag
  --event_id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: string -> string {
  let script = $in
  init-response

  mut attrs = []
  if ($auto_remove != false) { $attrs = ($attrs | append 'data-effect="el.remove()"') }
  if ($attributes != null) {
    for attr_name in ($attributes | columns) {
      let attr_value = $attributes | get $attr_name
      $attrs = ($attrs | append $'($attr_name)="($attr_value)"')
    }
  }

  let attrs_str = if ($attrs | length) > 0 { " " + ($attrs | str join " ") } else { "" }
  let script_tag = $"<script($attrs_str)>($script)</script>"

  mut data_lines = ["selector body" "mode append"]
  for line in ($script_tag | lines) { $data_lines = ($data_lines | append $"elements ($line)") }

  format-sse-event "datastar-patch-elements" $data_lines --event_id $event_id --retry $retry
}

# Parse signals from request (GET query `datastar` param or POST body JSON)
export def "from datastar-request" []: record -> record {
  let request = $in
  if $request.method == "POST" {
    try { $request.body | from json } catch { {} }
  } else {
    try { $request.query | get --optional datastar | default "{}" | from json } catch { {} }
  }
}
