# Datastar SSE SDK for Nushell
#
# Generates SSE event records for the Datastar hypermedia framework.
# Pipe output to `to sse` for streaming.
# Follows https://github.com/starfederation/datastar/blob/develop/sdk/ADR.md

# Conditionally apply a transform to pipeline input
def conditional-pipe [condition: bool, action: closure] {
  if $condition { do $action } else { $in }
}

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

  let data_lines = [
    (if $selector != null { $"selector ($selector)" })
    (if $mode != "outer" { $"mode ($mode)" })
    (if $use_view_transition { "useViewTransition true" })
    ...($elements | lines | each { $"elements ($in)" })
  ] | compact

  {event: "datastar-patch-elements", data: $data_lines}
  | conditional-pipe ($id != null) { insert id $id }
  | conditional-pipe ($retry != null) { insert retry $retry }
}

# Patch signals via SSE (JSON Merge Patch RFC 7386)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to dstar-patch-signal" [
  --only_if_missing # Only set signals missing on client
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: record -> record {
  let data_lines = [
    (if $only_if_missing { "onlyIfMissing true" })
    ...($in | to json --raw | lines | each { $"signals ($in)" })
  ] | compact

  {event: "datastar-patch-signals", data: $data_lines}
  | conditional-pipe ($id != null) { insert id $id }
  | conditional-pipe ($retry != null) { insert retry $retry }
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

  let attrs = [
    (if $auto_remove != false { 'data-effect="el.remove()"' })
    ...($attributes | default {} | transpose k v | each { $'($in.k)="($in.v)"' })
  ] | compact

  let attrs_str = $attrs | str join " " | if ($in | is-empty) { "" } else { $" ($in)" }
  let script_tag = $"<script($attrs_str)>($script)</script>"

  let data_lines = [
    "selector body"
    "mode append"
    ...($script_tag | lines | each { $"elements ($in)" })
  ]

  {event: "datastar-patch-elements", data: $data_lines}
  | conditional-pipe ($id != null) { insert id $id }
  | conditional-pipe ($retry != null) { insert retry $retry }
}

# Parse signals from request (GET query `datastar` param or POST body JSON)
export def "from datastar-request" []: record -> record {
  match $in.method {
    "POST" => (try { $in.body | from json } catch { {} }),
    _ => (try { $in.query.datastar? | default "{}" | from json } catch { {} })
  }
}
