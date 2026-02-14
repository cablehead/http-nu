use http-nu/datastar *
use http-nu/html *

# Run: http-nu --datastar :3001 examples/datastar-counter/serve.nu

{|req|
  HTML (HEAD (SCRIPT {type: "module" src: $DATASTAR_JS_PATH})) (BODY
    (DIV {"data-signals": "{count: 0}"}
      (SPAN {"data-text": "$count"} "0")
      (BUTTON {"data-on:click": "$count++"} "+1")
    )
  )
}
