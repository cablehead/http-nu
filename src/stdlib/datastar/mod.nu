# Datastar SSE SDK for Nushell
#
# Generates SSE event records for the Datastar hypermedia framework.
# Pipe output to `to sse` for streaming.
# Follows https://github.com/starfederation/datastar/blob/develop/sdk/ADR.md

export const DATASTAR_CDN_URL = "https://cdn.jsdelivr.net/gh/starfederation/datastar@1.0.2/bundles/datastar.js"
export const DATASTAR_JS_PATH = "/datastar@1.0.2.js"

export def SCRIPT-DATASTAR []: nothing -> record {
  {__html: $'<script type="module" src="($DATASTAR_JS_PATH)"></script>'}
}

# The bundle that includes Rocket, Datastar's web-component API, served under
# --datastar-rocket in place of the core bundle above. A page loads one or the
# other, never both: the Rocket bundle carries all of Datastar.
export const DATASTAR_ROCKET_CDN_URL = "https://cdn.jsdelivr.net/gh/starfederation/datastar@1.0.4/bundles/datastar-rocket.js"
export const DATASTAR_ROCKET_JS_PATH = "/datastar-rocket@1.0.4.js"

# Loads the bundle, after an import map that names it `datastar`. A component
# shipped as its own module (ptyZZZ's <pty-term>, say) then writes
# `import { rocket } from "datastar"` and shares this one copy, without knowing
# where the page serves it. The import map must come before any module script,
# so put this first in the head.
export def SCRIPT-DATASTAR-ROCKET []: nothing -> record {
  let imports = {imports: {datastar: $DATASTAR_ROCKET_JS_PATH}} | to json --raw
  {__html: $'<script type="importmap">($imports)</script><script type="module" src="($DATASTAR_ROCKET_JS_PATH)"></script>'}
}

# Patch HTML elements via SSE
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
# Modes: outer (default), inner, replace, prepend, append, before, after, remove
export def "to datastar-patch-elements" [
  --selector: string # CSS selector. If omitted, elements must have IDs
  --mode: string = "outer" # outer, inner, replace, prepend, append, before, after, remove
  --namespace: string # Content namespace: html (default), svg, or mathml
  --use-view-transition # Enable View Transitions API
  --id: string # SSE event ID
  --retry-duration: int # Retry interval in milliseconds
]: any -> record {
  let input = $in
  let type = $input | describe -d | get type
  let html = match $type {
    "string" => $input
    "record" => (if "__html" in $input { $input.__html } else { error make {msg: "record must have __html field"} })
    _ => (error make {msg: $"expected string or {__html} record, got ($type)"})
  }
  let data = [
    (if $selector != null { $"selector ($selector)" })
    (if $mode != "outer" { $"mode ($mode)" })
    (if $namespace != null { $"namespace ($namespace)" })
    (if $use_view_transition { "useViewTransition true" })
    ...($html | lines | each { $"elements ($in)" })
  ] | compact

  {event: "datastar-patch-elements" data: $data id: $id retry: $retry_duration}
}

# Patch signals via SSE (JSON Merge Patch RFC 7386)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
# Accepts a record (serialized to JSON) or a string (passed through as-is).
export def "to datastar-patch-signals" [
  --only-if-missing # Only set signals missing on client
  --id: string # SSE event ID
  --retry-duration: int # Retry interval in milliseconds
]: any -> record {
  let input = $in
  let json_str = match ($input | describe -d | get type) {
    "string" => $input
    _ => ($input | to json --raw)
  }
  let data = [
    (if $only_if_missing { "onlyIfMissing true" })
    ...($json_str | lines | each { $"signals ($in)" })
  ] | compact

  {event: "datastar-patch-signals" data: $data id: $id retry: $retry_duration}
}

# Execute JavaScript via SSE (appends <script> to body)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to datastar-execute-script" [
  --auto-remove = true # Remove script after execution
  --attributes: record # HTML attributes for script tag
  --id: string # SSE event ID
  --retry-duration: int # Retry interval in milliseconds
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

  {event: "datastar-patch-elements" data: $data id: $id retry: $retry_duration}
}

# Redirect via SSE (executes JavaScript to change window.location.href)
#
# Returns a record for `to sse`. Pipe the result to `to sse` for output.
export def "to datastar-redirect" []: string -> record {
  $"setTimeout\(\(\) => window.location.href = '($in)'\);" | to datastar-execute-script
}

# Parse signals from request (POST/PUT/PATCH body JSON, or query param for GET/DELETE)
# Usage: $in | from datastar-signals $req
export def "from datastar-signals" [req: record]: string -> record {
  match $req.method {
    "GET" | "DELETE" => (try { $req.query.datastar? | default "{}" | from json } catch { {} })
    _ => (try { $in | from json } catch { {} })
  }
}
