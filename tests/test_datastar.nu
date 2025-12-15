#!/usr/bin/env nu
# Tests for Datastar SSE SDK

use std/assert
use ../src/stdlib/datastar/mod.nu *

# Test basic SSE event formatting
def test_basic_sse_format [] {
  let result = "<div>test</div>" | to sse-patch-elements --selector "#target"

  assert ($result | str contains "event: datastar-patch-elements")
  assert ($result | str contains "data: selector #target")
  assert ($result | str contains "data: elements <div>test</div>")
}

# Test patch-elements with element by ID
def test_patch_elements_by_id [] {
  let html = "<div id=\"main\">Updated content</div>"
  let result = $html | to sse-patch-elements

  assert ($result | str contains "event: datastar-patch-elements")
  assert ($result | str contains "data: elements <div id=\"main\">Updated content</div>")
  # Default mode is outer, so it should not appear in output
  assert (not ($result | str contains "data: mode"))
}

# Test patch-elements with different merge modes
def test_patch_elements_modes [] {
  let prepend = "<div>content</div>" | to sse-patch-elements --selector "#target" --mode prepend
  assert ($prepend | str contains "data: mode prepend")

  let append = "<div>content</div>" | to sse-patch-elements --selector "#target" --mode append
  assert ($append | str contains "data: mode append")

  let before = "<div>content</div>" | to sse-patch-elements --selector "#target" --mode before
  assert ($before | str contains "data: mode before")

  let after = "<div>content</div>" | to sse-patch-elements --selector "#target" --mode after
  assert ($after | str contains "data: mode after")

  let remove = "" | to sse-patch-elements --selector "#target" --mode remove
  assert ($remove | str contains "data: mode remove")
  assert ($remove | str contains "data: selector #target")
}

# Test patch-elements with view transition
def test_patch_elements_transition [] {
  let result = "<div>content</div>" | to sse-patch-elements --selector "#target" --use_view_transition

  assert ($result | str contains "data: useViewTransition true")
}

# Test patch-signals with record input
def test_patch_signals_record [] {
  let signals = {count: 42, name: "Alice"}
  let result = $signals | to sse-patch-signals

  assert ($result | str contains "event: datastar-patch-signals")
  assert ($result | str contains "data: signals")
  assert ($result | str contains '"count":42')
  assert ($result | str contains '"name":"Alice"')
}

# Test patch-signals with only-if-missing flag
def test_patch_signals_only_if_missing [] {
  let result = {count: 5} | to sse-patch-signals --only_if_missing

  assert ($result | str contains "data: onlyIfMissing true")
}

# Test execute-script
def test_execute_script [] {
  let script = "console.log('Hello from Datastar')"
  let result = $script | to sse-execute-script

  # ExecuteScript uses patch-elements event
  assert ($result | str contains "event: datastar-patch-elements")
  assert ($result | str contains "data: selector body")
  assert ($result | str contains "data: mode append")
  assert ($result | str contains "<script")
  assert ($result | str contains "console.log")
  # Default auto_remove is true
  assert ($result | str contains 'data-effect="el.remove()"')
}

# Test execute-script without auto-remove
def test_execute_script_no_auto_remove [] {
  let result = "alert('test')" | to sse-execute-script --auto_remove false

  assert ($result | str contains "<script>alert('test')</script>")
  assert (not ($result | str contains "data-effect"))
}

# Test execute-script with attributes
def test_execute_script_attributes [] {
  let result = "doThing()" | to sse-execute-script --attributes {type: "module"}

  assert ($result | str contains 'type="module"')
  assert ($result | str contains "doThing()")
}

# Test SSE event-id
def test_event_id [] {
  let result = "<div>content</div>" | to sse-patch-elements --selector "#target" --event_id "msg-123"

  assert ($result | str contains "id: msg-123")
}

# Test SSE retry
def test_retry [] {
  let result = {count: 1} | to sse-patch-signals --retry 5000

  assert ($result | str contains "retry: 5000")
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

# Run all tests
def main [] {
  test_basic_sse_format
  test_patch_elements_by_id
  test_patch_elements_modes
  test_patch_elements_transition
  test_patch_signals_record
  test_patch_signals_only_if_missing
  test_execute_script
  test_execute_script_no_auto_remove
  test_execute_script_attributes
  test_event_id
  test_retry
  test_from_datastar_request_query
  test_from_datastar_request_post
  test_from_datastar_request_empty
}
