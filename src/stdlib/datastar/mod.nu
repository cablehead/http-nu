# Datastar SSE SDK for Nushell
#
# This module provides utilities for generating Server-Sent Events (SSE)
# compatible with the Datastar hypermedia framework.
#
# Datastar uses SSE to deliver DOM updates, signal patches, and script execution
# to the browser without full page reloads.

# Internal helper to format SSE events
#
# Formats data according to Server-Sent Events specification (RFC 8895).
# Each field is prefixed with its type (event:, data:, id:, retry:).
# Multi-line data values are split into multiple "data:" lines.
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

# Convert HTML elements to SSE patch-elements event
#
# Sends complete HTML elements for DOM manipulation.
# Per ADR: elements must be complete, well-formed HTML (not fragments).
#
# # Patch Modes (ElementPatchMode)
# - outer (default): Morph entire element, preserving state
# - inner: Morph inner HTML only, preserving state
# - replace: Replace entire element, reset state
# - prepend: Insert at beginning inside target
# - append: Insert at end inside target
# - before: Insert before target element
# - after: Insert after target element
# - remove: Remove target element from DOM
export def "to sse-patch-elements" [
  --selector: string # CSS selector for target. If not provided, elements must have IDs
  --mode: string = "outer" # Patch mode: outer, inner, replace, prepend, append, before, after, remove
  --use_view_transition # Enable View Transitions API
  --event_id: string # SSE event ID
  --retry: int # SSE retry interval in milliseconds
]: string -> string {
  let elements = $in

  # Build data lines per ADR spec (only include non-defaults)
  mut data_lines = []

  # Selector (if provided)
  if ($selector != null) {
    $data_lines = ($data_lines | append $"selector ($selector)")
  }

  # Mode (if not default 'outer')
  if $mode != "outer" {
    $data_lines = ($data_lines | append $"mode ($mode)")
  }

  # View transition (if enabled)
  if $use_view_transition {
    $data_lines = ($data_lines | append "useViewTransition true")
  }

  # Elements (each line prefixed with 'elements ')
  # For remove mode, elements can be omitted
  if ($elements | str length) > 0 {
    for line in ($elements | lines) {
      $data_lines = ($data_lines | append $"elements ($line)")
    }
  }

  format-sse-event "datastar-patch-elements" $data_lines --event_id $event_id --retry $retry
}

# Convert record to SSE patch-signals event
#
# Sends signal updates using JSON Merge Patch (RFC 7386).
# Merges the provided record into the client's signal store.
export def "to sse-patch-signals" [
  --only_if_missing # Only set signals that don't exist on client
  --event_id: string # SSE event ID
  --retry: int # SSE retry interval in milliseconds
]: record -> string {
  let signals = $in

  # Build data lines per ADR spec
  mut data_lines = []

  # onlyIfMissing (only if true)
  if $only_if_missing {
    $data_lines = ($data_lines | append "onlyIfMissing true")
  }

  # Signals (JSON on separate lines)
  let json_str = $signals | to json --raw
  for line in ($json_str | lines) {
    $data_lines = ($data_lines | append $"signals ($line)")
  }

  format-sse-event "datastar-patch-signals" $data_lines --event_id $event_id --retry $retry
}

# Execute JavaScript via SSE
#
# Per ADR: Sends <script> tag via datastar-patch-elements event, not datastar-execute-script.
# The script tag is appended to body with optional auto-remove and attributes.
export def "to sse-execute-script" [
  --auto_remove = true # Remove script tag after execution (default: true)
  --attributes: record # HTML attributes for script tag (e.g., {type: "module"})
  --event_id: string # SSE event ID
  --retry: int # SSE retry interval in milliseconds
]: string -> string {
  let script = $in

  # Build script tag
  mut attrs = []

  # Add auto-remove via data-effect if enabled (default true)
  if ($auto_remove != false) {
    $attrs = ($attrs | append 'data-effect="el.remove()"')
  }

  # Add custom attributes
  if ($attributes != null) {
    for attr_name in ($attributes | columns) {
      let attr_value = $attributes | get $attr_name
      $attrs = ($attrs | append $'($attr_name)="($attr_value)"')
    }
  }

  let attrs_str = if ($attrs | length) > 0 {
    " " + ($attrs | str join " ")
  } else {
    ""
  }

  let script_tag = $"<script($attrs_str)>($script)</script>"

  # Build data lines per ADR spec
  # ExecuteScript uses patch-elements event with selector=body, mode=append
  mut data_lines = [
    "selector body"
    "mode append"
  ]

  # Add script tag as elements
  for line in ($script_tag | lines) {
    $data_lines = ($data_lines | append $"elements ($line)")
  }

  format-sse-event "datastar-patch-elements" $data_lines --event_id $event_id --retry $retry
}

# Parse Datastar signals from HTTP request
#
# Extracts signal data from GET query parameters or POST body.
# For GET requests, looks for "datastar" query parameter with JSON.
# For POST requests, parses the entire body as JSON.
#
# Returns an empty record if no signals are found.
export def "from datastar-request" []: record -> record {
  let request = $in

  if $request.method == "POST" {
    # POST: parse body as JSON
    try {
      $request.body | from json
    } catch {
      {}
    }
  } else {
    # GET: parse "datastar" query parameter
    try {
      $request.query | get --optional datastar | default "{}" | from json
    } catch {
      {}
    }
  }
}
