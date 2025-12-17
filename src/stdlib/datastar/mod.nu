# Datastar SSE SDK for Nushell
#
# Generates SSE event records for the Datastar hypermedia framework.
# Pipe output to `to sse` for streaming.
# Follows https://github.com/starfederation/datastar/blob/develop/sdk/ADR.md

export const DATASTAR_CDN_URL = "https://cdn.jsdelivr.net/gh/starfederation/datastar@1.0.0-RC.7/bundles/datastar.js"

# Patch HTML elements via SSE
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
# Modes: outer (default), inner, replace, prepend, append, before, after, remove
export def "to dstar-patch-element" [
  --selector: string # CSS selector. If omitted, elements must have IDs
  --mode: string = "outer" # outer, inner, replace, prepend, append, before, after, remove
  --namespace: string # Content namespace: html (default) or svg
  --use_view_transition # Enable View Transitions API
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: string -> record {
  let data = [
    (if $selector != null { $"selector ($selector)" })
    (if $mode != "outer" { $"mode ($mode)" })
    (if $namespace != null { $"namespace ($namespace)" })
    (if $use_view_transition { "useViewTransition true" })
    ...($in | lines | each { $"elements ($in)" })
  ] | compact

  {event: "datastar-patch-elements" data: $data id: $id retry: $retry}
}

# Patch signals via SSE (JSON Merge Patch RFC 7386)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to dstar-patch-signal" [
  --only_if_missing # Only set signals missing on client
  --id: string # SSE event ID
  --retry: int # Retry interval in milliseconds
]: record -> record {
  let data = [
    (if $only_if_missing { "onlyIfMissing true" })
    ...($in | to json --raw | lines | each { $"signals ($in)" })
  ] | compact

  {event: "datastar-patch-signals" data: $data id: $id retry: $retry}
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

  let data = [
    "selector body"
    "mode append"
    ...($script_tag | lines | each { $"elements ($in)" })
  ]

  {event: "datastar-patch-elements" data: $data id: $id retry: $retry}
}

# Parse signals from request (GET query `datastar` param or POST body JSON)
# Usage: $in | from datastar-request $req
export def "from datastar-request" [req: record]: string -> record {
  match $req.method {
    "POST" => (try { $in | from json } catch { {} })
    _ => (try { $req.query.datastar? | default "{}" | from json } catch { {} })
  }
}
