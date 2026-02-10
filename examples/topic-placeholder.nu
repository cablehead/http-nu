# topic-placeholder.nu - Default handler served when --topic is used
# but the topic doesn't exist yet. Also works as a standalone demo:
#
#   http-nu :3001 examples/topic-placeholder.nu

use http-nu/html *

let topic = "__TOPIC__"
let store_path = "__STORE_PATH__"

{|req|
  match $req.path {
    "/request" => {
      HTML (
        HEAD (TITLE "http-nu")
      ) (
        BODY
          (H1 (A {href: "/"} "http-nu"))
          (H2 "request")
          (PRE ($req | reject headers | to yaml))
          (H2 "headers")
          (PRE ($req.headers | to yaml))
      )
    }

    _ => {
      HTML (
        HEAD (TITLE "http-nu")
      ) (
        BODY
          (H1 "http-nu")
          (P $"Waiting for topic " (CODE $topic) " ...")
          (P "Append a handler closure to start serving:")
          (PRE $"'{|req| \"hello, world\"}' | xs append ($store_path)/sock ($topic)")
          (P "With " (CODE "-w") ", the server will automatically reload when the topic is updated.")
          (HR)
          (P (A {href: "/request"} "request info"))
      ) | metadata set --merge {'http.response': {status: 503}}
    }
  }
}
