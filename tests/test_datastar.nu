#!/usr/bin/env nu
# Tests for Datastar SSE SDK

use std/assert
use ../src/stdlib/datastar/mod.nu *

# Stub for `to sse` (actual implementation is in http-nu Rust code)
# Skips null values for id, retry, event, data fields
def "to sse" []: record -> string {
  let rec = $in
  mut out = ""
  if ($rec.event? | is-not-empty) { $out = $out + $"event: ($rec.event)\n" }
  if ($rec.id? | is-not-empty) { $out = $out + $"id: ($rec.id)\n" }
  if ($rec.retry? | is-not-empty) { $out = $out + $"retry: ($rec.retry)\n" }
  if ($rec.data? | is-not-empty) {
    let data = $rec.data
    let lines = if ($data | describe | str starts-with "list") {
      $data | each { $in | to text | lines } | flatten
    } else {
      $data | lines
    }
    for line in $lines { $out = $out + $"data: ($line)\n" }
  }
  $out + "\n"
}

# Test patch-elements returns correct record structure
def test_patch_elements_record [] {
  let result = "<div>test</div>" | to datastar-patch-elements --selector "#target"

  assert equal $result.event "datastar-patch-elements"
  assert ("selector #target" in $result.data)
  assert ("elements <div>test</div>" in $result.data)
}

# Test patch-elements with element by ID (no selector)
def test_patch_elements_by_id [] {
  let html = r#'<div id="main">Updated content</div>'#
  let result = $html | to datastar-patch-elements

  assert equal $result.event "datastar-patch-elements"
  assert (r#'elements <div id="main">Updated content</div>'# in $result.data)
  # Default mode is outer, so it should not appear in data
  assert (not ($result.data | any { $in | str starts-with "mode" }))
}

# Test patch-elements with different merge modes
def test_patch_elements_modes [] {
  let prepend = "<div>content</div>" | to datastar-patch-elements --selector "#target" --mode prepend
  assert ("mode prepend" in $prepend.data)

  let append = "<div>content</div>" | to datastar-patch-elements --selector "#target" --mode append
  assert ("mode append" in $append.data)

  let before = "<div>content</div>" | to datastar-patch-elements --selector "#target" --mode before
  assert ("mode before" in $before.data)

  let after = "<div>content</div>" | to datastar-patch-elements --selector "#target" --mode after
  assert ("mode after" in $after.data)

  let remove = "" | to datastar-patch-elements --selector "#target" --mode remove
  assert ("mode remove" in $remove.data)
  assert ("selector #target" in $remove.data)
}

# Test patch-elements with view transition
def test_patch_elements_transition [] {
  let result = "<div>content</div>" | to datastar-patch-elements --selector "#target" --use-view-transition

  assert ("useViewTransition true" in $result.data)
}

# Test patch-elements namespace option
def test_patch_elements_namespace [] {
  # Default namespace (html) should not appear in data
  let html = "<div>content</div>" | to datastar-patch-elements --selector "#target"
  assert (not ($html.data | any { $in | str starts-with "namespace" }))

  # SVG namespace should appear
  let svg = "<circle cx=\"50\" cy=\"50\" r=\"40\"/>" | to datastar-patch-elements --selector "#target" --namespace svg
  assert ("namespace svg" in $svg.data)
}

# Test patch-signals with record input
def test_patch_signals_record [] {
  let signals = {count: 42 name: "Alice"}
  let result = $signals | to datastar-patch-signals

  assert equal $result.event "datastar-patch-signals"
  assert ($result.data | any { str starts-with "signals" })
  assert ($result.data | any { str contains '"count":42' })
  assert ($result.data | any { str contains '"name":"Alice"' })
}

# Test patch-signals with only-if-missing flag
def test_patch_signals_only_if_missing [] {
  let result = {count: 5} | to datastar-patch-signals --only-if-missing

  assert ("onlyIfMissing true" in $result.data)
}

# Test patch-signals with raw string input (multiline)
def test_patch_signals_raw_string [] {
  let raw = "{\n\"one\": 1,\n\"two\": 2}"
  let result = $raw | to datastar-patch-signals

  assert equal $result.event "datastar-patch-signals"
  assert equal ($result.data | length) 3
  assert equal ($result.data | get 0) "signals {"
  assert equal ($result.data | get 1) "signals \"one\": 1,"
  assert equal ($result.data | get 2) "signals \"two\": 2}"
}

# Test execute-script
def test_execute_script [] {
  let script = "console.log('Hello from Datastar')"
  let result = $script | to datastar-execute-script

  # ExecuteScript uses patch-elements event
  assert equal $result.event "datastar-patch-elements"
  assert ("selector body" in $result.data)
  assert ("mode append" in $result.data)
  assert ($result.data | any { str contains "<script" })
  assert ($result.data | any { str contains "console.log" })
  # Default auto-remove is true
  assert ($result.data | any { str contains r#'data-effect="el.remove()"'# })
}

# Test execute-script without auto-remove
def test_execute_script_no_auto_remove [] {
  let result = "alert('test')" | to datastar-execute-script --auto-remove false

  assert ($result.data | any { str contains "<script>alert('test')</script>" })
  assert (not ($result.data | any { str contains "data-effect" }))
}

# Test execute-script with attributes
def test_execute_script_attributes [] {
  let result = "doThing()" | to datastar-execute-script --attributes {type: "module"}

  assert ($result.data | any { str contains r#'type="module"'# })
  assert ($result.data | any { str contains "doThing()" })
}

# Test SSE id field
def test_id_field [] {
  let result = "<div>content</div>" | to datastar-patch-elements --selector "#target" --id "msg-123"

  assert equal $result.id "msg-123"
}

# Test SSE retry-duration field
def test_retry_duration_field [] {
  let result = {count: 1} | to datastar-patch-signals --retry-duration 5000

  assert equal $result.retry 5000
}

# Test from datastar-signals with query string
def test_from_datastar_signals_query [] {
  let req = {
    method: "GET"
    query: {datastar: '{"count":42,"active":true}'}
  }

  let signals = "" | from datastar-signals $req
  assert equal $signals.count 42
  assert equal $signals.active true
}

# Test from datastar-signals with POST body
def test_from_datastar_signals_post [] {
  let req = {method: "POST"}
  let body = '{"username":"alice","score":100}'

  let signals = $body | from datastar-signals $req
  assert equal $signals.username "alice"
  assert equal $signals.score 100
}

# Test from datastar-signals with empty signals
def test_from_datastar_signals_empty [] {
  let req = {
    method: "GET"
    query: {}
  }

  let signals = "" | from datastar-signals $req
  assert equal $signals {}
}

# Test piping to `to sse` produces valid SSE output
def test_to_sse_integration [] {
  let result = "<div>test</div>" | to datastar-patch-elements --selector "#target" | to sse

  assert ($result | str contains "event: datastar-patch-elements")
  assert ($result | str contains "data: selector #target")
  assert ($result | str contains "data: elements <div>test</div>")
}

# Test `to sse` with id and retry-duration fields
def test_to_sse_with_id_retry_duration [] {
  let result = {count: 1} | to datastar-patch-signals --id "evt-1" --retry-duration 3000 | to sse

  assert ($result | str contains "id: evt-1")
  assert ($result | str contains "retry: 3000")
  assert ($result | str contains "event: datastar-patch-signals")
}

# Test redirect helper
def test_redirect [] {
  let result = "/dashboard" | to datastar-redirect

  assert equal $result.event "datastar-patch-elements"
  assert ("selector body" in $result.data)
  assert ("mode append" in $result.data)
  assert ($result.data | any { str contains "window.location.href = '/dashboard'" })
  assert ($result.data | any { str contains "setTimeout" })
}

# Run all tests
def main [] {
  test_patch_elements_record
  test_patch_elements_by_id
  test_patch_elements_modes
  test_patch_elements_transition
  test_patch_elements_namespace
  test_patch_signals_record
  test_patch_signals_only_if_missing
  test_patch_signals_raw_string
  test_execute_script
  test_execute_script_no_auto_remove
  test_execute_script_attributes
  test_id_field
  test_retry_duration_field
  test_from_datastar_signals_query
  test_from_datastar_signals_post
  test_from_datastar_signals_empty
  test_to_sse_integration
  test_to_sse_with_id_retry_duration
  test_redirect
}
