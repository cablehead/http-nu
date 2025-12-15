# Datastar SSE SDK for Nushell
#
# Generates SSE event records for the Datastar hypermedia framework.
# Pipe output to `to sse` for streaming.
# Follows https://github.com/starfederation/datastar/blob/develop/sdk/ADR.md

# Patch HTML elements via SSE
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
# Modes: outer (default), inner, replace, prepend, append, before, after, remove
export def "to dstar-patch-element" [
  --selector: string # CSS selector. If omitted, elements must have IDs
  --mode: string = "outer" # outer, inner, replace, prepend, append, before, after, remove
  --use_view_transition # Enable View Transitions API
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: string -> record {
  let elements = $in

  mut data_lines = []
  if ($selector != null) { $data_lines = ($data_lines | append $"selector ($selector)") }
  if $mode != "outer" { $data_lines = ($data_lines | append $"mode ($mode)") }
  if $use_view_transition { $data_lines = ($data_lines | append "useViewTransition true") }
  if ($elements | str length) > 0 {
    for line in ($elements | lines) { $data_lines = ($data_lines | append $"elements ($line)") }
  }

  mut rec = {event: "datastar-patch-elements", data: ($data_lines | str join "\n")}
  if ($id != null) { $rec = ($rec | insert id $id) }
  if ($retry != null) { $rec = ($rec | insert retry $retry) }
  $rec
}

# Patch signals via SSE (JSON Merge Patch RFC 7386)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to dstar-patch-signal" [
  --only_if_missing # Only set signals missing on client
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: record -> record {
  let signals = $in

  mut data_lines = []
  if $only_if_missing { $data_lines = ($data_lines | append "onlyIfMissing true") }
  let json_str = $signals | to json --raw
  for line in ($json_str | lines) { $data_lines = ($data_lines | append $"signals ($line)") }

  mut rec = {event: "datastar-patch-signals", data: ($data_lines | str join "\n")}
  if ($id != null) { $rec = ($rec | insert id $id) }
  if ($retry != null) { $rec = ($rec | insert retry $retry) }
  $rec
}

# Execute JavaScript via SSE (appends <script> to body)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to dstar-execute-script" [
  --auto_remove = true # Remove script after execution
  --attributes: record # HTML attributes for script tag
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: string -> record {
  let script = $in

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

  mut data_lines = ["selector body", "mode append"]
  for line in ($script_tag | lines) { $data_lines = ($data_lines | append $"elements ($line)") }

  mut rec = {event: "datastar-patch-elements", data: ($data_lines | str join "\n")}
  if ($id != null) { $rec = ($rec | insert id $id) }
  if ($retry != null) { $rec = ($rec | insert retry $retry) }
  $rec
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
