# Datastar SDK test endpoint
# Run: http-nu :7331 examples/datastar-sdk-test/serve.nu
# Test: datastar-sdk-tests -server http://localhost:7331

use http-nu/router *
use http-nu/datastar *

def handle-event [event: record] {
  match $event.type {
    "patchElements" => {
      let elements = $event.elements? | default ""
      let selector = $event.selector?
      let mode = $event.mode? | default "outer"
      let vt = $event.useViewTransition? | default false
      let id = $event.eventId?
      let retry = $event.retryDuration?
      if $vt {
        $elements | to dstar-patch-element --selector $selector --mode $mode --use_view_transition --id $id --retry $retry
      } else {
        $elements | to dstar-patch-element --selector $selector --mode $mode --id $id --retry $retry
      }
    }
    "patchSignals" => {
      let signals = $event.signals-raw? | default ($event.signals? | default {})
      let oim = $event.onlyIfMissing? | default false
      let id = $event.eventId?
      let retry = $event.retryDuration?
      if $oim {
        $signals | to dstar-patch-signal --only_if_missing --id $id --retry $retry
      } else {
        $signals | to dstar-patch-signal --id $id --retry $retry
      }
    }
    "executeScript" => {
      let script = $event.script? | default ""
      let auto_remove = $event.autoRemove? | default true
      let attributes = $event.attributes?
      let id = $event.eventId?
      let retry = $event.retryDuration?
      $script | to dstar-execute-script --auto_remove $auto_remove --attributes $attributes --id $id --retry $retry
    }
    _ => { error make {msg: $"unknown event type: ($event.type)"} }
  }
}

{|req|
  dispatch $req [
    (
      route {path: "/test"} {|req ctx|
        let input = $in | from datastar-request $req
        let events = $input.events? | default []
        $events | each {|event| handle-event $event } | to sse
      }
    )
  ]
}
