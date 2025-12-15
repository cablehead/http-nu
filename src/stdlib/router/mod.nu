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
#   [
#     # Exact path match - use record
#     (route
#       {path: "/health"}
#       {|req ctx| "OK"})
#
#     # Method + exact path - use record
#     (route
#       {method: "POST", path: "/users"}
#       {|req ctx| .response {status: 201}; "User created"})
#
#     # Path parameters - use special key path-matches
#     (route
#       {path-matches: "/users/:id"}
#       {|req ctx| $"User: ($ctx.id)"})
#
#     # Multiple path parameters
#     (route
#       {path-matches: "/users/:userId/posts/:postId"}
#       {|req ctx| $"User ($ctx.userId) post ($ctx.postId)"})
#
#     # Header matching - use special key has-header
#     (route
#       {has-header: {accept: "application/json"}}
#       {|req ctx| {status: "ok"}})
#
#     # Combined: method + path params + headers
#     (route
#       {
#         method: "POST"
#         path-matches: "/api/:version/data"
#         has-header: {accept: "application/json"}
#       }
#       {|req ctx| {version: $ctx.version, status: "ok"}})
#
#     # Complex logic - use closure escape hatch
#     (route
#       {|r| if ($r.method == "DELETE") { {} }}
#       {|req ctx| .response {status: 405}; "Deletes not allowed"})
#
#     # Fallback (always matches)
#     (route
#       true
#       {|req ctx| .response {status: 404}; "Not Found"})
#   ]
#   | dispatch $req
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
  let test_fn = if $test == true {
    # Always match - return empty context
    {|req| {} }
  } else if (($test | describe) | str starts-with "record") {
    # Convert record pattern to a test closure
    {|req|
      mut context = {}
      let pattern = $test

      # Process each key in the pattern
      for key in ($pattern | columns) {
        if $key == "path-matches" {
          # Special: path parameter extraction
          let params = $req | path-matches ($pattern | get $key)
          if $params == null {
            return null
          }
          $context = ($context | merge $params)
        } else if $key == "has-header" {
          # Special: header checking
          let headers_to_check = $pattern | get $key
          for header_name in ($headers_to_check | columns) {
            let header_value = $headers_to_check | get $header_name
            if not ($req | has-header $header_name $header_value) {
              return null
            }
          }
        } else {
          # Regular key: exact equality check
          let expected = $pattern | get $key
          let actual = $req | get --optional $key
          if $actual != $expected {
            return null
          }
        }
      }

      $context
    }
  } else {
    # Already a closure
    $test
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
  let request = $in
  let path = ($request.path | str trim --right --char '/')

  # Split pattern and path into segments
  let pattern_segments = ($pattern | str trim --right --char '/' | split row '/')
  let path_segments = ($path | split row '/')

  # Must have same number of segments
  if ($pattern_segments | length) != ($path_segments | length) {
    return null
  }

  # Match each segment and extract parameters
  mut params = {}
  mut index = 0

  for pattern_seg in $pattern_segments {
    let path_seg = ($path_segments | get $index)

    if ($pattern_seg | str starts-with ':') {
      # Extract parameter name (remove leading ':')
      let param_name = ($pattern_seg | str substring 1..)
      $params = ($params | insert $param_name $path_seg)
    } else if $pattern_seg != $path_seg {
      # Exact segment doesn't match
      return null
    }

    $index += 1
  }

  $params
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
  let request = $in
  let normalized_name = ($header_name | str downcase)

  # Get all header names (case-insensitive check)
  let header_exists = (
    $request.headers
    | columns
    | any {|col| ($col | str downcase) == $normalized_name }
  )

  if not $header_exists {
    return false
  }

  # Get the header value (case-insensitive)
  let header_value = (
    $request.headers
    | transpose name value
    | where ($it.name | str downcase) == $normalized_name
    | get value.0
    | default ""
  )

  # Split on comma for comma-separated values and check each part
  $header_value
  | split row ","
  | each {|v| $v | str trim }
  | any {|v| $v == $value }
}

# Dispatch a request through a list of routes
#
# Finds the first matching route and executes its handler.
# Returns the result from the matched route handler.
#
# Routes are tested in order until one matches (returns non-null context).
# The matched route's handler receives the request and extracted context.
@example "dispatch to matching route" {
  let routes = [
    (route {path: "/health"} {|req ctx| "OK" })
    (route true {|req ctx| "fallback" })
  ]
  $routes | dispatch {path: "/health"}
} --result "OK"
@example "dispatch to fallback" {
  let routes = [
    (route {path: "/health"} {|req ctx| "OK" })
    (route true {|req ctx| "fallback" })
  ]
  $routes | dispatch {path: "/unknown"}
} --result "fallback"
@example "dispatch with extracted params" {
  let routes = [
    (route {path-matches: "/users/:id"} {|req ctx| $"User: ($ctx.id)" })
  ]
  $routes | dispatch {path: "/users/123"}
} --result "User: 123"
export def dispatch [
  request: record # The HTTP request record to route
]: list -> any {
  let routes = $in

  let matched = $routes
  | each {|rt|
    let ctx = do $rt.test $request
    if $ctx != null { {route: $rt ctx: $ctx req: $request} }
  }
  | compact
  | first

  if $matched == null {
    # Set 501 status if .response command is available (in http-nu context)
    # In standalone test context, this will silently skip
    try { .response {status: 501} }
    "No route configured"
  } else {
    do $matched.route.handle $matched.req $matched.ctx
  }
}
