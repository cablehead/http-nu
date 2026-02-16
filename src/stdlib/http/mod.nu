# Cookie utilities for http-nu
#
# Secure defaults: HttpOnly, SameSite=Lax, Secure (unless --dev mode)

# Parse cookies from a request record
#
# Usage: $req | cookie parse
export def "cookie parse" []: record -> record {
  $in.headers.cookie?
  | default ""
  | split row "; "
  | where $it != ""
  | each {|pair|
    let parts = $pair | split row "=" --number 2
    if ($parts | length) == 2 {
      {($parts.0 | str trim): ($parts.1 | str trim)}
    } else {
      {}
    }
  }
  | reduce --fold {} {|it, acc| $acc | merge $it}
}

# Set a cookie on the response
#
# Threads the pipeline value through, accumulating Set-Cookie headers in metadata.
# Defaults: Path=/, HttpOnly, SameSite=Lax, Secure (in prod mode)
#
# Usage: "OK" | cookie set "session" "abc123" --max-age 86400 | cookie set "theme" "dark"
export def "cookie set" [
  name: string
  value: string
  --max-age: int              # Cookie lifetime in seconds
  --path: string = "/"        # Cookie path
  --domain: string            # Cookie domain
  --no-httponly               # Allow JavaScript access
  --no-secure                 # Omit Secure flag even in prod mode
  --same-site: string = "Lax" # SameSite policy: Lax, Strict, or None
]: any -> any {
  metadata set {|m|
    let header = ([
      $"($name)=($value)"
      $"Path=($path)"
      (if not $no_httponly { "HttpOnly" })
      (if (not $env.HTTP_NU.dev) and (not $no_secure) { "Secure" })
      $"SameSite=($same_site)"
      (if $max_age != null { $"Max-Age=($max_age)" })
      (if $domain != null { $"Domain=($domain)" })
    ] | compact | str join "; ")
    let resp = $m | get -i "http.response" | default {}
    let headers = $resp | get -i headers | default {}
    let cookies = $headers | get -i Set-Cookie | default [] | append $header
    $m | upsert "http.response" ($resp | upsert headers ($headers | upsert Set-Cookie $cookies))
  }
}

# Delete a cookie from the client
#
# Threads the pipeline value through, setting Max-Age=0 to expire the cookie.
# Path and domain must match the original cookie.
#
# Usage: "OK" | cookie delete "session"
export def "cookie delete" [
  name: string
  --path: string = "/"   # Must match the path used when setting the cookie
  --domain: string       # Must match the domain used when setting the cookie
]: any -> any {
  metadata set {|m|
    let header = ([
      $"($name)="
      $"Path=($path)"
      "Max-Age=0"
      (if $domain != null { $"Domain=($domain)" })
    ] | compact | str join "; ")
    let resp = $m | get -i "http.response" | default {}
    let headers = $resp | get -i headers | default {}
    let cookies = $headers | get -i Set-Cookie | default [] | append $header
    $m | upsert "http.response" ($resp | upsert headers ($headers | upsert Set-Cookie $cookies))
  }
}
