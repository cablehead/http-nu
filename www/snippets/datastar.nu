{|req ctx|
  let signals = from datastar-request $req
  let interval = $signals.interval? | default 100 | into int

  "Hello from Datastar 🚀"
  | split chars
  | generate {|c acc = ""|
    sleep ($interval * 1ms)
    let acc = $acc + $c
    {
      out: (
        DIV {id: "message"} $acc
      )
      next: $acc
    }
  }
  # instruct datastar to patch our new HTML into place
  | each { to dstar-patch-element }
  # re-enable the button by setting running back to false
  | append ({running: false} | to dstar-patch-signal)
  | to sse
}
