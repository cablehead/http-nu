#!/usr/bin/env nu

use std/assert
use ../src/stdlib/router/mod.nu *

# Testing route command with closures
let r = route {|req| {} } {|req ctx| "result" }
assert equal ($r | columns | sort) [handle test]

let ctx = do $r.test {path: "/anything"}
assert equal $ctx {}

let r2 = route {|req| {name: "alice"} } {|req ctx| $"Hello ($ctx.name)" }
let result = do $r2.handle {method: "GET"} {name: "bob"}
assert equal $result "Hello bob"

# Testing route command with records
let r3 = route {method: "POST" path: "/users"} {|req ctx| "created" }
assert equal (do $r3.test {method: "POST" path: "/users" query: {}}) {}
assert equal (do $r3.test {method: "GET" path: "/users" query: {}}) null
assert equal (do $r3.test {method: "POST" path: "/posts" query: {}}) null

let r4 = route {method: "GET"} {|req ctx| "any GET" }
assert equal (do $r4.test {method: "GET" path: "/health" query: {}}) {}
assert equal (do $r4.test {method: "GET" path: "/users" query: {}}) {}
assert equal (do $r4.test {method: "POST" path: "/health" query: {}}) null

let r5 = route {path: "/health"} {|req ctx| "health check" }
assert equal (do $r5.test {method: "GET" path: "/health" query: {}}) {}
assert equal (do $r5.test {method: "POST" path: "/health" query: {}}) {}
assert equal (do $r5.test {method: "GET" path: "/users" query: {}}) null

# Testing route with path-matches special key
let r6 = route {path-matches: "/users/:id"} {|req ctx| $ctx.id }
let ctx6 = do $r6.test {path: "/users/123"}
assert equal $ctx6 {id: "123"}
assert equal (do $r6.test {path: "/posts/123"}) null

# Testing combined method + path-matches
let r7 = route {method: "GET" path-matches: "/users/:id"} {|req ctx| $ctx }
assert equal (do $r7.test {method: "GET" path: "/users/456"}) {id: "456"}
assert equal (do $r7.test {method: "POST" path: "/users/456"}) null

# Testing has-header special key
let r8 = route {has-header: {accept: "application/json"}} {|req ctx| "ok" }
assert equal (do $r8.test {headers: {accept: "application/json"}}) {}
assert equal (do $r8.test {headers: {accept: "text/html"}}) null

# Testing multiple headers with has-header
let r9 = route {has-header: {accept: "application/json" "content-type": "application/json"}} {|req ctx| "ok" }
assert equal (do $r9.test {headers: {accept: "application/json" "content-type": "application/json"}}) {}
assert equal (do $r9.test {headers: {accept: "application/json"}}) null

# Testing everything combined
let r10 = route {
  method: "POST"
  path-matches: "/api/:version/data"
  has-header: {accept: "application/json"}
} {|req ctx| $ctx }
assert equal (
  do $r10.test {
    method: "POST"
    path: "/api/v1/data"
    headers: {accept: "application/json"}
  }
) {version: "v1"}
assert equal (
  do $r10.test {
    method: "GET"
    path: "/api/v1/data"
    headers: {accept: "application/json"}
  }
) null

# Testing empty pattern (fallback)
let r11 = route {} {|req ctx| "404" }
assert equal (do $r11.test {method: "GET" path: "/anything"}) {}

# Testing true (always matches)
let r12 = route true {|req ctx| "always" }
assert equal (do $r12.test {method: "GET" path: "/anything"}) {}
assert equal (do $r12.test {method: "POST" path: "/other"}) {}

# Testing path-matches
assert equal ({path: "/users"} | path-matches "/users") {}
assert equal ({path: "/posts"} | path-matches "/users") null
assert equal ({path: "/users/123"} | path-matches "/users/:id") {id: "123"}
assert equal ({path: "/users/alice/posts/456"} | path-matches "/users/:userId/posts/:postId") {userId: "alice" postId: "456"}
assert equal ({path: "/users/123/comments/789"} | path-matches "/users/:id/posts/:postId") null
assert equal ({path: "/users/"} | path-matches "/users") {}

# Testing has-header
assert equal ({headers: {accept: "application/json"}} | has-header "accept" "application/json") true
assert equal ({headers: {accept: "text/html"}} | has-header "Accept" "text/html") true
assert equal ({headers: {"Content-Type": "application/json"}} | has-header "content-type" "application/json") true
assert equal ({headers: {accept: "text/html, application/json"}} | has-header "accept" "application/json") true
assert equal ({headers: {accept: "text/html, application/json, text/xml"}} | has-header "accept" "text/xml") true
assert equal ({headers: {accept: "text/html, application/json"}} | has-header "accept" "text/xml") false
assert equal ({headers: {host: "localhost"}} | has-header "authorization" "Bearer token") false
assert equal ({headers: {accept: "application/json"}} | has-header "accept" "text/html") false
assert equal ({headers: {accept: "text/html,application/json"}} | has-header "accept" "application/json") true

# Testing dispatch command
let routes = [
  (route {path: "/health"} {|req ctx| "OK" })
  (route {path-matches: "/users/:id"} {|req ctx| $"User: ($ctx.id)" })
  (route true {|req ctx| "404" })
]

assert equal ($routes | dispatch {method: "GET" path: "/health"}) "OK"
assert equal ($routes | dispatch {method: "GET" path: "/users/alice"}) "User: alice"
assert equal ($routes | dispatch {method: "GET" path: "/unknown"}) "404"

# Testing dispatch with method matching
let routes2 = [
  (route {method: "POST" path: "/users"} {|req ctx| "created" })
  (route {method: "GET" path: "/users"} {|req ctx| "list" })
  (route true {|req ctx| "404" })
]

assert equal ($routes2 | dispatch {method: "POST" path: "/users"}) "created"
assert equal ($routes2 | dispatch {method: "GET" path: "/users"}) "list"
assert equal ($routes2 | dispatch {method: "DELETE" path: "/users"}) "404"

# Testing dispatch with headers
let routes3 = [
  (route {has-header: {accept: "application/json"}} {|req ctx| {status: "ok"} })
  (route true {|req ctx| "fallback" })
]

assert equal ($routes3 | dispatch {headers: {accept: "application/json"}}) {status: "ok"}
assert equal ($routes3 | dispatch {headers: {accept: "text/html"}}) "fallback"

# Testing dispatch with combined conditions
let routes4 = [
  (
    route {
      method: "POST"
      path-matches: "/api/:version/data"
      has-header: {accept: "application/json"}
    } {|req ctx| {version: $ctx.version status: "ok"} }
  )
  (route true {|req ctx| "fallback" })
]

assert equal (
  $routes4 | dispatch {
    method: "POST"
    path: "/api/v1/data"
    headers: {accept: "application/json"}
  }
) {version: "v1" status: "ok"}
assert equal (
  $routes4 | dispatch {
    method: "GET"
    path: "/api/v1/data"
    headers: {accept: "application/json"}
  }
) "fallback"

# Testing dispatch with no matching routes
# Note: In standalone test we can't check .response metadata,
# but when embedded in http-nu it will set status: 501
let routes5 = [
  (route {method: "POST" path: "/users"} {|req ctx| "created" })
]

let result = $routes5 | dispatch {method: "GET" path: "/health"}
assert equal $result "No route configured"
