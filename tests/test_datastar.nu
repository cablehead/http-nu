#!/usr/bin/env nu
# Tests for Datastar SSE SDK

use std/assert
use ../src/stdlib/datastar/mod.nu *

# Stub for `to sse` (actual implementation is in http-nu Rust code)
def "to sse" []: record -> string {
  let rec = $in
  mut out = ""
  if ($rec.id? != null) { $out = $out + $"id: ($rec.id)\n" }
  if ($rec.retry? != null) { $out = $out + $"retry: ($rec.retry)\n" }
  if ($rec.event? != null) { $out = $out + $"event: ($rec.event)\n" }
  if ($rec.data? != null) {
    for line in ($rec.data | lines) { $out = $out + $"data: ($line)\n" }
  }
  $out + "\n"
}

# Test patch-element returns correct record structure
def test_patch_element_record [] {
  let result = "<div>test</div>" | to dstar-patch-element --selector "#target"

  assert equal $result.event "datastar-patch-elements"
  assert ($result.data | str contains "selector #target")
  assert ($result.data | str contains "elements <div>test</div>")
}

# Test patch-element with element by ID (no selector)
def test_patch_element_by_id [] {
  let html = "<div id=\"main\">Updated content</div>"
  let result = $html | to dstar-patch-element

  assert equal $result.event "datastar-patch-elements"
  assert ($result.data | str contains "elements <div id=\"main\">Updated content</div>")
  # Default mode is outer, so it should not appear in data
  assert (not ($result.data | str contains "mode"))
}

# Test patch-element with different merge modes
def test_patch_element_modes [] {
  let prepend = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode prepend
  assert ($prepend.data | str contains "mode prepend")

  let append = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode append
  assert ($append.data | str contains "mode append")

  let before = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode before
  assert ($before.data | str contains "mode before")

  let after = "<div>content</div>" | to dstar-patch-element --selector "#target" --mode after
  assert ($after.data | str contains "mode after")

  let remove = "" | to dstar-patch-element --selector "#target" --mode remove
  assert ($remove.data | str contains "mode remove")
  assert ($remove.data | str contains "selector #target")
}

# Test patch-element with view transition
def test_patch_element_transition [] {
  let result = "<div>content</div>" | to dstar-patch-element --selector "#target" --use_view_transition

  assert ($result.data | str contains "useViewTransition true")
}

# Test patch-signal with record input
def test_patch_signal_record [] {
  let signals = {count: 42, name: "Alice"}
  let result = $signals | to dstar-patch-signal

  assert equal $result.event "datastar-patch-signals"
  assert ($result.data | str contains "signals")
  assert ($result.data | str contains '"count":42')
  assert ($result.data | str contains '"name":"Alice"')
}

# Test patch-signal with only-if-missing flag
def test_patch_signal_only_if_missing [] {
  let result = {count: 5} | to dstar-patch-signal --only_if_missing

  assert ($result.data | str contains "onlyIfMissing true")
}

# Test execute-script
def test_execute_script [] {
  let script = "console.log('Hello from Datastar')"
  let result = $script | to dstar-execute-script

  # ExecuteScript uses patch-elements event
  assert equal $result.event "datastar-patch-elements"
  assert ($result.data | str contains "selector body")
  assert ($result.data | str contains "mode append")
  assert ($result.data | str contains "<script")
  assert ($result.data | str contains "console.log")
  # Default auto_remove is true
  assert ($result.data | str contains 'data-effect="el.remove()"')
}

# Test execute-script without auto-remove
def test_execute_script_no_auto_remove [] {
  let result = "alert('test')" | to dstar-execute-script --auto_remove false

  assert ($result.data | str contains "<script>alert('test')</script>")
  assert (not ($result.data | str contains "data-effect"))
}

# Test execute-script with attributes
def test_execute_script_attributes [] {
  let result = "doThing()" | to dstar-execute-script --attributes {type: "module"}

  assert ($result.data | str contains 'type="module"')
  assert ($result.data | str contains "doThing()")
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
