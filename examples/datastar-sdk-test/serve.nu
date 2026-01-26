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
        $elements | to datastar-patch-elements --selector $selector --mode $mode --use-view-transition --id $id --retry-duration $retry
      } else {
        $elements | to datastar-patch-elements --selector $selector --mode $mode --id $id --retry-duration $retry
      }
    }
    "patchSignals" => {
      let signals = $event.signals-raw? | default ($event.signals? | default {})
      let oim = $event.onlyIfMissing? | default false
      let id = $event.eventId?
      let retry = $event.retryDuration?
      if $oim {
        $signals | to datastar-patch-signals --only-if-missing --id $id --retry-duration $retry
      } else {
        $signals | to datastar-patch-signals --id $id --retry-duration $retry
      }
    }
    "executeScript" => {
      let script = $event.script? | default ""
      let auto_remove = $event.autoRemove? | default true
      let attributes = $event.attributes?
      let id = $event.eventId?
      let retry = $event.retryDuration?
      $script | to datastar-execute-script --auto-remove $auto_remove --attributes $attributes --id $id --retry-duration $retry
    }
    _ => { error make {msg: $"unknown event type: ($event.type)"} }
  }
}

{|req|
  dispatch $req [
    (
      route {path: "/test"} {|req ctx|
        let input = $in | from datastar-signals $req
        let events = $input.events? | default []
        $events | each {|event| handle-event $event } | to sse
      }
    )
  ]
}
