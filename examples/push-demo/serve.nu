# push-demo — Web Push + Notifications + a2hs demo.
#
# Routes:
#   GET  /                        index.html (template-expanded)
#   GET  /manifest.json           PWA manifest
#   GET  /sw.js                   service worker (Cache-Control: no-cache)
#   GET  /state.js                browser-side state machine
#   GET  /icons/*                 static PNGs
#   GET  /vapid-public-key        VAPID pubkey for applicationServerKey
#   POST /subscribe               persist a PushSubscription to xs
#   POST /unsubscribe             mark endpoint as user-unsubscribed
#   POST /send-self               admin: send a test push to all active subs
#   GET  /health                  readiness probe (used by mise dev task)
#
# Run via:
#   mise run push-demo:dev
# Or directly:
#   VAPID_PRIVATE_KEY=<b64>  VAPID_PUBLIC_KEY=<b64>  VAPID_SUBJECT=mailto:you@x.com \
#   ./target/debug/http-nu --plugin ./target/debug/nu_plugin_push \
#     --store ./.store/push-demo :8080 examples/push-demo/serve.nu

use ./lib/subs.nu *

const SCRIPT_DIR = path self | path dirname
const WWW_DIR    = $SCRIPT_DIR | path join "www"

# Per-server-restart cache buster for /state.js (CDN-busts on deploy).
$env.STATIC_REV = (random uuid | str substring 0..7)

print $"push-demo: started — VAPID_SUBJECT=($env.VAPID_SUBJECT? | default '<unset>') subs=(current_subs_count)"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def static-file [path: string, content_type: string, --no-cache] {
  let body = (open --raw ($WWW_DIR | path join $path))
  let headers = (
    if $no_cache {
      { "Content-Type": $content_type, "Cache-Control": "no-cache, no-store, must-revalidate" }
    } else {
      { "Content-Type": $content_type, "Cache-Control": $"public, max-age=300" }
    }
  )
  $body | metadata set { merge {'http.response': { status: 200, headers: $headers }}}
}

def json-response [obj: any, --status (-s) = 200] {
  $obj | to json | metadata set { merge {'http.response': {
    status: $status,
    headers: { "Content-Type": "application/json" }
  }}}
}

def text-response [text: string, --status (-s) = 200] {
  $text | metadata set { merge {'http.response': {
    status: $status,
    headers: { "Content-Type": "text/plain; charset=utf-8" }
  }}}
}

def bad-request [msg: string] {
  text-response --status 400 $"bad request: ($msg)"
}

def is-same-origin [req: record] {
  let origin = ($req.headers | get -i origin)
  let host   = ($req.headers | get -i host)
  if $origin == null { return true }    # curl, server-side, fine
  # `url parse | get host` strips the port, so compare the full origin string
  # against http/https + host header (which keeps the port).
  ($origin == $"http://($host)") or ($origin == $"https://($host)")
}

def admin-authorized [req: record] {
  let token = ($env.PUSH_ADMIN_TOKEN? | default "")
  if ($token | is-empty) { return false }
  let auth = ($req.headers | get -i authorization | default "")
  $auth == $"Bearer ($token)"
}

# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

{|req|
  let body = $in

  match $req {
    {method: "GET", path: "/"} => {
      let html = (open --raw ($WWW_DIR | path join "index.html"))
      let expanded = ($html | str replace --all "__STATIC_REV__" $env.STATIC_REV)
      $expanded | metadata set { merge {'http.response': {
        status: 200,
        headers: { "Content-Type": "text/html; charset=utf-8" }
      }}}
    }

    {method: "GET", path: "/manifest.json"} => {
      static-file "manifest.json" "application/manifest+json"
    }

    # Cache-Control: no-cache is critical here. Cloudflare's default edge
    # cache will pin old service workers and your push code stops updating.
    {method: "GET", path: "/sw.js"} => {
      static-file "sw.js" "application/javascript" --no-cache
    }

    {method: "GET", path: "/state.js"} => {
      # No-cache so iterating on device detection / state machine doesn't get
      # pinned to a stale Safari cache. Trivially fetched (~6KB).
      static-file "state.js" "application/javascript" --no-cache
    }

    {method: "GET", path: $p} if ($p | str starts-with "/icons/") => {
      let name = ($p | str substring 7..)   # strip "/icons/"
      if ($name | str contains "/") or ($name | str contains "..") {
        bad-request "icon name"
      } else {
        let file = ($WWW_DIR | path join "icons" $name)
        if not ($file | path exists) {
          text-response --status 404 "icon not found"
        } else {
          (open --raw $file) | metadata set { merge {'http.response': {
            status: 200,
            headers: { "Content-Type": "image/png", "Cache-Control": "public, max-age=3600" }
          }}}
        }
      }
    }

    {method: "GET", path: "/health"} => {
      text-response "ok"
    }

    {method: "GET", path: "/vapid-public-key"} => {
      text-response ($env.VAPID_PUBLIC_KEY? | default "")
    }

    {method: "POST", path: "/subscribe"} => {
      if not (is-same-origin $req) {
        bad-request "origin"
      } else {
        # $body is already a string for text Content-Types; coerce defensively
        # in case the caller sent binary.
        let json = ($body | into string)
        # Wrap the parse in a result-tuple so the catch can't short-circuit the
        # match arm's response value. `return` inside `try/catch` doesn't behave
        # like an early-exit in nushell -- it just returns the catch value.
        let res = (try {
          { ok: true, value: (push subscription parse $json) }
        } catch {|e|
          { ok: false, error: $e.msg }
        })
        if not $res.ok {
          bad-request $"subscription: ($res.error)"
        } else {
          # Stash the whole sub record in --meta. Avoids a CAS read step and the
          # subscription is small (sub-1KB) -- well within meta-friendly size.
          null | .append "push.subscription.added" --meta $res.value
          json-response { ok: true, endpoint: $res.value.endpoint }
        }
      }
    }

    {method: "POST", path: "/unsubscribe"} => {
      let payload = (try { $body | into string | from json } catch { null })
      if $payload == null or ($payload.endpoint? | is-empty) {
        bad-request "endpoint required"
      } else {
        null | .append "push.subscription.unsubscribed" --meta {
          endpoint: $payload.endpoint,
        }
        json-response { ok: true }
      }
    }

    # Admin: fan-out a test push to every current subscriber.
    # Auth via Bearer PUSH_ADMIN_TOKEN. In production replace with proper
    # session auth -- the demo's just a starting point.
    {method: "POST", path: "/send-self"} => {
      let subs = (current_subs)
      if ($subs | is-empty) {
        json-response --status 404 { error: "no active subscriptions" }
      } else {
        let payload = {
          title: "Hello from push-demo",
          body: $"Sent at (date now | format date '%H:%M:%S') to ($subs | length) subscribers",
          tag: "self-test",
        } | to json -r
        let results = ($subs | push send $payload --ttl 60 --urgency normal)
        # Convert any 410 results into expired events so the next current_subs
        # projection excludes them. This closes the loop.
        $results | where result == "expired" | each {|r|
          null | .append "push.subscription.expired" --meta { endpoint: $r.endpoint }
        } | ignore
        json-response { sent: ($results | length), results: $results }
      }
    }

    {method: "POST", path: "/send"} => {
      if not (admin-authorized $req) {
        text-response --status 401 "unauthorized"
      } else {
        let payload = ($body | into string)
        let subs = (current_subs)
        let results = ($subs | push send $payload)
        $results | where result == "expired" | each {|r|
          null | .append "push.subscription.expired" --meta { endpoint: $r.endpoint }
        } | ignore
        json-response { sent: ($results | length), results: $results }
      }
    }

    _ => {
      text-response --status 404 "not found"
    }
  }
}
