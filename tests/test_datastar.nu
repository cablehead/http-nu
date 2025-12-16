#!/usr/bin/env nu
# Tests for Datastar SSE SDK

use std/assert
use ../src/stdlib/datastar/mod.nu *

# Stub for `to sse` (actual implementation is in http-nu Rust code)
# Skips null values for id, retry, event, data fields
def "to sse" []: record -> string {
  let rec = $in
  mut out = ""
  if ($rec.id? | is-not-empty) { $out = $out + $"id: ($rec.id)\n" }
  if ($rec.retry? | is-not-empty) { $out = $out + $"retry: ($rec.retry)\n" }
  if ($rec.event? | is-not-empty) { $out = $out + $"event: ($rec.event)\n" }
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

# Test patch-element returns correct record structure
def test_patch_element_record [] {
  let result = "<div>test</div>" | to dstar-patch-element --selector "#target"

  assert equal $result.event "datastar-patch-elements"
  assert ("selector #target" in $result.data)
  assert ("elements <div>test</div>" in $result.data)
}

# Test patch-element with element by ID (no selector)
def test_patch_element_by_id [] {
  let html = "<div id=\"main\">Updated content</div>"
  let result = $html | to dstar-patch-element

  assert equal $result.event "datastar-patch-elements"
  assert ("elements <div id=\"main\">Updated content</div>" in $result.data)
  # Default mode is outer, so it should not appear in data
  assert (not ($result.data | any { $in | str starts-with "mode" }))
}

# Test patch-element with different merge modes
def test_patch_element_modes [] {
  let prepend = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode prepend
  assert ("mode prepend" in $prepend.data)

  let append = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode append
  assert ("mode append" in $append.data)

  let before = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode before
  assert ("mode before" in $before.data)

  let after = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode after
  assert ("mode after" in $after.data)

  let remove = "" | to dstar-patch-element --selector "#target" --mode remove
  assert ("mode remove" in $remove.data)
  assert ("selector #target" in $remove.data)
}

# Test patch-element with view transition
def test_patch_element_transition [] {
  let result = "<div>content</div>" | to dstar-patch-element --selector "#target" --use_view_transition

  assert ("useViewTransition true" in $result.data)
}

# Test patch-signal with record input
def test_patch_signal_record [] {
  let signals = {count: 42, name: "Alice"}
  let result = $signals | to dstar-patch-signal

  assert equal $result.event "datastar-patch-signals"
  assert ($result.data | any { str starts-with "signals" })
  assert ($result.data | any { str contains '"count":42' })
  assert ($result.data | any { str contains '"name":"Alice"' })
}

# Test patch-signal with only-if-missing flag
def test_patch_signal_only_if_missing [] {
  let result = {count: 5} | to dstar-patch-signal --only_if_missing

  assert ("onlyIfMissing true" in $result.data)
}

# Test execute-script
def test_execute_script [] {
  let script = "console.log('Hello from Datastar')"
  let result = $script | to dstar-execute-script

  # ExecuteScript uses patch-elements event
  assert equal $result.event "datastar-patch-elements"
  assert ("selector body" in $result.data)
  assert ("mode append" in $result.data)
  assert ($result.data | any { str contains "<script" })
  assert ($result.data | any { str contains "console.log" })
  # Default auto_remove is true
  assert ($result.data | any { str contains 'data-effect="el.remove()"' })
}

# Test execute-script without auto-remove
def test_execute_script_no_auto_remove [] {
  let result = "alert('test')" | to dstar-execute-script --auto_remove false

  assert ($result.data | any { str contains "<script>alert('test')</script>" })
  assert (not ($result.data | any { str contains "data-effect" }))
}

# Test execute-script with attributes
def test_execute_script_attributes [] {
  let result = "doThing()" | to dstar-execute-script --attributes {type: "module"}

  assert ($result.data | any { str contains 'type="module"' })
  assert ($result.data | any { str contains "doThing()" })
}

# Test SSE id field
def test_id_field [] {
  let result = "<div>content</div>" | to dstar-patch-element --selector "#target" --id "msg-123"

  assert equal $result.id "msg-123"
}

# Test SSE retry field
def test_retry_field [] {
  let result = {count: 1} | to dstar-patch-signal --retry 5000

  assert equal $result.retry 5000
}

# Test from datastar-request with query string
def test_from_datastar_request_query [] {
  let req = {
    method: "GET"
    query: {datastar: '{"count":42,"active":true}'}
  }

  let signals = $req | from datastar-request
  assert equal $signals.count 42
  assert equal $signals.active true
}

# Test from datastar-request with POST body
def test_from_datastar_request_post [] {
  let req = {
    method: "POST"
    body: '{"username":"alice","score":100}'
  }

  let signals = $req | from datastar-request
  assert equal $signals.username "alice"
  assert equal $signals.score 100
}

# Test from datastar-request with empty signals
def test_from_datastar_request_empty [] {
  let req = {
    method: "GET"
    query: {}
  }

  let signals = $req | from datastar-request
  assert equal $signals {}
}

# Test piping to `to sse` produces valid SSE output
def test_to_sse_integration [] {
  let result = "<div>test</div>" | to dstar-patch-element --selector "#target" | to sse

  assert ($result | str contains "event: datastar-patch-elements")
  assert ($result | str contains "data: selector #target")
  assert ($result | str contains "data: elements <div>test</div>")
}

# Test `to sse` with id and retry fields
def test_to_sse_with_id_retry [] {
  let result = {count: 1} | to dstar-patch-signal --id "evt-1" --retry 3000 | to sse

  assert ($result | str contains "id: evt-1")
  assert ($result | str contains "retry: 3000")
  assert ($result | str contains "event: datastar-patch-signals")
}

# Run all tests
def main [] {
  test_patch_element_record
  test_patch_element_by_id
  test_patch_element_modes
  test_patch_element_transition
  test_patch_signal_record
  test_patch_signal_only_if_missing
  test_execute_script
  test_execute_script_no_auto_remove
  test_execute_script_attributes
  test_id_field
  test_retry_field
  test_from_datastar_request_query
  test_from_datastar_request_post
  test_from_datastar_request_empty
  test_to_sse_integration
  test_to_sse_with_id_retry
}
