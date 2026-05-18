# Session auth, on top of cross.stream frames.
#
# Cookie (secret) -> session frame -> user_id (public)
#   session=<token>  ->  session.<token> {meta: {user_id}}  ->  user_id
#
# - <token> is a CSPRNG-backed `random uuid` (cookie secret; never
#   stamped on any other frame).
# - <session_id> is the *frame id* of the session frame -- a SCRU128,
#   auditable, public-safe. Stamped on write frames (moves) so per-
#   session activity can be correlated without revealing the cookie.
# - <user_id> is the stable, public identity. Lives at /by/<user_id>.
#
# Old `player=<uuid>` cookies are honoured ONCE: the first request from
# such a visitor mints a fresh session bound to that user_id, then the
# legacy cookie is cleared. After that, only `session` is read.

use http-nu/http *

# Resolve an incoming request to a session record, claiming a legacy
# player cookie if that is all we have.
#
# Returns either null (anonymous) or:
#   {user_id, session_id, token, fresh: bool, claimed: bool}
#
# - fresh = true  when this call just minted the session frame
# - claimed = true when we minted FROM a legacy `player` cookie (callers
#   should pipe responses through `session-cookies set` to install the
#   new session cookie and delete the legacy one)
export def resolve-session [req: record]: nothing -> any {
  let cookies = $req | cookie parse
  let token = $cookies | get session? | default ""
  if ($token | is-not-empty) {
    let frame = try { .last $"session.($token)" } catch { null }
    if $frame != null {
      let user_id = $frame.meta | get user_id? | default ""
      if ($user_id | is-not-empty) {
        return {
          user_id: $user_id
          session_id: $frame.id
          token: $token
          fresh: false
          claimed: false
        }
      }
    }
  }
  let legacy = $cookies | get player? | default ""
  if ($legacy | is-not-empty) {
    let minted = mint-session $legacy
    return ($minted | upsert claimed true)
  }
  null
}

# Mint a fresh session bound to a user_id. Appends a `session.<token>`
# frame (ttl: last:1 so only the active binding is kept). Returns
# {token, session_id, user_id, fresh: true, claimed: false}.
export def mint-session [user_id: string]: nothing -> record {
  let token = random uuid
  let frame = (null | .append $"session.($token)" --meta {user_id: $user_id} --ttl last:1)
  {
    token: $token
    session_id: $frame.id
    user_id: $user_id
    fresh: true
    claimed: false
  }
}

# Pipeline helper. Sets the `session` cookie (1yr sliding window) and
# clears any legacy `player` cookie on the response. Implicit pipeline
# input (no `$in`) so http.response metadata threads through cleanly --
# `$in` collects the value and seems to strip the merged-metadata
# instruction set upstream.
export def "session-cookies set" [session: record]: any -> any {
  cookie set "session" $session.token --max-age 31536000
  | cookie delete "player"
}
