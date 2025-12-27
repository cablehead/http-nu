# HTTP routing utilities for nushell
#
# This module provides utilities for building declarative HTTP routers.
# Routes consist of a test closure that returns a context record (or null)
# and a handler closure that receives the request and context.
#
# # Example
#
# ```nu
# use router.nu *
#
# {|req|
#   dispatch $req [
#     # Exact path match - use record
#     (route {path: "/health"} {|req ctx| "OK"})
#
#     # Method + exact path - use record
#     (route {method: "POST", path: "/users"} {|req ctx|
#       .response {status: 201}
#       "User created"
#     })
#
#     # Path parameters - use special key path-matches
#     (route {path-matches: "/users/:id"} {|req ctx| $"User: ($ctx.id)"})
#
#     # Header matching - use special key has-header
#     (route {has-header: {accept: "application/json"}} {|req ctx| {status: "ok"}})
#
#     # Fallback (always matches)
#     (route true {|req ctx| .response {status: 404}; "Not Found"})
#   ]
# }
# ```

# Create a route record with a test and handler
#
# The test parameter can be:
# - A record pattern with regular and special keys:
#   - Regular keys (method, path, etc.): exact equality match
#   - Special key 'path-matches': extracts path parameters to context
#   - Special key 'has-header': checks multiple headers
# - A closure that returns a context record or null (escape hatch for complex logic)
# - The boolean true (always matches - useful for fallback routes)
#
# The handler closure receives the request and context record.
# Context contains extracted path parameters from path-matches.
@example "record test - exact match" {
  let r = route {method: "POST" path: "/users"} {|req ctx| "created" }
  do $r.test {method: "POST" path: "/users" query: {}}
} --result {}
@example "record with path-matches" {
  let r = route {path-matches: "/users/:id"} {|req ctx| $ctx.id }
  let ctx = do $r.test {path: "/users/123"}
  $ctx.id
} --result "123"
@example "record with has-header" {
  let r = route {has-header: {accept: "application/json"}} {|req ctx| "ok" }
  do $r.test {headers: {accept: "application/json"}}
} --result {}
@example "combined special keys" {
  let r = route {method: "POST" path-matches: "/api/:v/data"} {|req ctx| $ctx.v }
  let ctx = do $r.test {method: "POST" path: "/api/v1/data"}
  $ctx.v
} --result "v1"
@example "true always matches" {
  let r = route true {|req ctx| "fallback" }
  do $r.test {method: "GET" path: "/anything"}
} --result {}
@example "closure test with context" {
  let r = route {|req| {id: "123"} } {|req ctx| $ctx.id }
  do $r.handle {method: "GET"} {id: "456"}
} --result "456"
export def route [
  test: any # Record (supports special keys), closure, or true (always match)
  handle: closure # Handler closure that receives request and context
]: nothing -> record<test: closure, handle: closure> {
  let test_fn = match ($test | describe) {
    "bool" => {|req| {} }
    $t if ($t | str starts-with "record") => {|req|
      let pattern = $test
      # Process pattern keys, accumulating context or returning null on mismatch
      $pattern | columns | reduce --fold {} {|key ctx|
        if $ctx == null { return null }
        match $key {
          "path-matches" => {
            let params = $req | path-matches ($pattern | get $key)
            if $params == null { null } else { $ctx | merge $params }
          }
          "has-header" => {
            let headers = $pattern | get $key
            let all_match = $headers | columns | all {|h| $req | has-header $h ($headers | get $h) }
            if $all_match { $ctx } else { null }
          }
          _ => {
            if ($req | get -o $key) == ($pattern | get $key) { $ctx } else { null }
          }
        }
      }
    }
    _ => $test # Already a closure
  }

  {test: $test_fn handle: $handle}
}

# Match a path pattern with parameter extraction
#
# Returns a record with extracted parameters if the path matches,
# or null if it doesn't match.
#
# Supported patterns:
# - Exact match: "/users"
# - Single parameter: "/users/:id"
# - Multiple parameters: "/users/:userId/posts/:postId"
# - Parameter must match a path segment (no partial matches)
@example "exact match returns empty record" {
  {path: "/users"} | path-matches "/users"
} --result {}
@example "exact match fails" {
  {path: "/posts"} | path-matches "/users"
} --result null
@example "single parameter extraction" {
  {path: "/users/123"} | path-matches "/users/:id"
} --result {id: "123"}
@example "multiple parameters" {
  {path: "/users/123/posts/456"} | path-matches "/users/:userId/posts/:postId"
} --result {userId: "123" postId: "456"}
@example "parameter mismatch returns null" {
  {path: "/users/123/comments/789"} | path-matches "/users/:id/posts/:postId"
} --result null
@example "trailing slash handling" {
  {path: "/users/"} | path-matches "/users"
} --result {}
export def path-matches [
  pattern: string # Path pattern with optional :param segments
]: record -> record {
  let path = ($in.path | str trim --right --char '/')
  let pattern_segments = ($pattern | str trim --right --char '/' | split row '/')
  let path_segments = ($path | split row '/')

  if ($pattern_segments | length) != ($path_segments | length) {
    return null
  }

  # Zip and process segments, returning null on mismatch or extracted params
  $pattern_segments
  | zip $path_segments
  | reduce --fold {} {|pair params|
    if $params == null { return null }
    match ($pair.0 | str starts-with ':') {
      true => ($params | insert ($pair.0 | str substring 1..) $pair.1)
      false => (if $pair.0 == $pair.1 { $params } else { null })
    }
  }
}

# Check if a request header contains a specific value
#
# Returns true if the header exists and contains the specified value.
# Header name matching is case-insensitive (per HTTP spec).
# Handles comma-separated values (e.g., Accept: text/html, application/json).
#
# Note: Due to http-nu's current implementation, only the first header
# is captured if multiple headers with the same name are sent.
@example "exact header value match" {
  {headers: {accept: "application/json"}} | has-header "accept" "application/json"
} --result true
@example "case-insensitive header name" {
  {headers: {accept: "text/html"}} | has-header "Accept" "text/html"
} --result true
@example "comma-separated values" {
  {headers: {accept: "text/html, application/json"}} | has-header "accept" "application/json"
} --result true
@example "value not in list" {
  {headers: {accept: "text/html, application/json"}} | has-header "accept" "text/xml"
} --result false
@example "missing header" {
  {headers: {host: "localhost"}} | has-header "authorization" "Bearer token"
} --result false
export def has-header [
  header_name: string # Header name to check (case-insensitive)
  value: string # Value to look for in the header
]: record -> bool {
  let normalized_name = ($header_name | str downcase)

  $in.headers
  | transpose name value
  | where { $in.name | str downcase | $in == $normalized_name }
  | get -o value.0
  | default ""
  | split row ","
  | any { $in | str trim | $in == $value }
}

# Find the first matching route for a request
#
# Returns {route: <route>, ctx: <context>} for the first match.
# Appends internal 501 fallback so result is never null.
def find-match [
  request: record
  routes: list
]: nothing -> record {
  let fallback = route true {|req ctx|
    try { .response {status: 501} }
    "No route configured"
  }

  $routes
  | append $fallback
  | each {|rt| do $rt.test $request | if $in != null { {route: $rt ctx: $in} } }
  | compact
  | first
}

# Execute matched route - `do` must be first to receive $in
def dispatch-execute [
  match: record
  request: record
]: any -> any {
  do $match.route.handle $request $match.ctx
}

# Dispatch a request through a list of routes
#
# Finds the first matching route and executes its handler.
# The request body ($in) streams through to the matched handler.
#
# Routes are tested in order until one matches (returns non-null context).
# The matched route's handler receives the request, context, and body as $in.
@example "dispatch to matching route" {
  let routes = [
    (route {path: "/health"} {|req ctx| "OK" })
    (route true {|req ctx| "fallback" })
  ]
  dispatch {path: "/health"} $routes
} --result "OK"
@example "dispatch to fallback" {
  let routes = [
    (route {path: "/health"} {|req ctx| "OK" })
    (route true {|req ctx| "fallback" })
  ]
  dispatch {path: "/unknown"} $routes
} --result "fallback"
@example "dispatch with extracted params" {
  let routes = [
    (route {path-matches: "/users/:id"} {|req ctx| $"User: ($ctx.id)" })
  ]
  dispatch {path: "/users/123"} $routes
} --result "User: 123"
export def dispatch [
  request: record # The HTTP request record to route
  routes: list # List of route records to match against
]: any -> any {
  # Single expression so $in flows through to handler
  dispatch-execute (find-match $request $routes) $request
}
