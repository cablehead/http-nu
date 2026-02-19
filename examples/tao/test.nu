use std/assert

const script_dir = path self | path dirname

let handler = source ($script_dir | path join serve.nu)
let response = do $handler {method: GET, path: "/", headers: {}}
assert ($response | str contains "<h1>State in the Right Place</h1>")
