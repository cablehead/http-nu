# Session-based authentication for cedar-admin.
#
# Shape borrowed from examples/2048/auth.nu (cookie -> xs session frame ->
# user_id), but two big changes:
#   1. mint-session takes an explicit user_id INSTEAD of generating a random
#      UUID. user_ids must exist in seed/users.csv with status=ACTIVE.
#   2. Adds lookup-user / is-admin / is-active backed by seed/users.csv +
#      seed/user_statuses.csv + seed/roles.csv. No magic; if the user is not in
#      users.csv, auth fails. This is real identity, not anonymous tokens.
#
# Token semantics (same as 2048):
#   - <token> is a CSPRNG-backed `random uuid`, stored in the `session` cookie.
#     Never stamped on any non-session frame.
#   - <session_id> is the SCRU128 frame id -- auditable, safe to log/display.
#   - <user_id> is the stable identity (e.g. `usr_admin_001`); lives in
#     users.csv. The same user can have multiple sessions across browsers.
#
# Storage:
#   session.<token>  {meta: {user_id}}  --ttl last:1
#
# `last:1` keeps exactly one binding per token (so re-minting overwrites
# rather than appends). Logout = clear cookie. The xs frame sits there but
# nothing references it.

use http-nu/http *

# Read users.csv once per call. Cheap for 12 rows; cedar-admin is a
# single-host demo. PO edits to users.csv take effect on the very next request.
def load-users [seed_dir: path] {
  open ($seed_dir | path join "users.csv")
}

# Look up a user by id from users.csv. Returns the row record or null.
export def lookup-user [
  user_id: string
  seed_dir: path
] {
  load-users $seed_dir | where id == $user_id | get -i 0 | default null
}

# True if the user exists in users.csv AND status_code is "ACTIVE".
# user_statuses.csv defines valid status codes (ACTIVE, PENDING_APPROVAL, etc.);
# we treat only ACTIVE as eligible to authz. PENDING accounts can sign in
# (so they see "your account is pending approval") but cannot be granted.
export def is-active [
  user_id: string
  seed_dir: path
] {
  let u = (lookup-user $user_id $seed_dir)
  if $u == null { return false }
  $u.status_code == "ACTIVE"
}

# True if the user's role_code is "ADMIN" (per seed/roles.csv).
export def is-admin [
  user_id: string
  seed_dir: path
] {
  let u = (lookup-user $user_id $seed_dir)
  if $u == null { return false }
  $u.role_code == "ADMIN"
}

# Compact projection of users.csv for the login dropdown: id, role_code,
# status_code, name_en. Filtered to status=ACTIVE so disabled accounts
# don't appear (PENDING_APPROVAL users can still log in via direct user_id
# POST -- they just won't be in the dropdown).
export def list-users-for-login [
  seed_dir: path
] {
  load-users $seed_dir
  | where status_code == "ACTIVE"
  | select id role_code name_en
}

# Resolve an incoming request to a session. Returns either null (anonymous,
# no cookie or stale cookie) or:
#   {user_id, session_id, token, fresh: false}
export def resolve-session [
  req: record
] {
  let cookies = ($req | cookie parse)
  let token = ($cookies | get session? | default "")
  if ($token | is-empty) { return null }

  let frame = (.last $"session.($token)")
  if $frame == null { return null }

  let user_id = ($frame.meta | get user_id? | default "")
  if ($user_id | is-empty) { return null }

  {
    user_id:    $user_id
    session_id: $frame.id
    token:      $token
    fresh:      false
  }
}

# Mint a fresh session bound to a known user_id. Caller MUST validate the
# user exists in users.csv first (this function does not check, by design --
# /login validates, /admin-as does too). Appends a `session.<token>` frame
# with --ttl last:1 so the topic holds exactly one binding.
#
# Returns {token, session_id, user_id, fresh: true}.
export def mint-session [
  user_id: string
] {
  let token = (random uuid)
  let frame = (null | .append $"session.($token)" --meta {user_id: $user_id} --ttl last:1)
  {
    token:      $token
    session_id: $frame.id
    user_id:    $user_id
    fresh:      true
  }
}

# Pipeline helper. Installs the `session` cookie with a 1-year sliding
# window. Use as the final step of a /login handler.
export def "session-cookies set" [
  session: record
]: any -> any {
  cookie set "session" $session.token --max-age 31536000
}

# Pipeline helper for /logout. Clears the cookie. The xs session frame is
# left in place (nothing references it without the cookie).
export def "session-cookies clear" []: any -> any {
  cookie delete "session"
}
