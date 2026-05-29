# http-nu/cedar -- middleware over the nu_plugin_cedar plugin.
#
# Setup in your serve.nu:
#   plugin add target/debug/nu_plugin_cedar
#   plugin use cedar
#   use http-nu/cedar *
#   $env.CEDAR_POLICIES = open policies.cedar
#
# Then in route handlers:
#   let auth = cedar check {
#     principal: $"User::\"($session.user_id)\""
#     action:    'Action::"EDIT_POLICY"'
#     resource:  'Platform::"global"'
#     entities:  ($my_entities | to json -r)   # optional, defaults to "[]"
#   }
#   if not $auth.allow { return (cedar forbidden $auth) }
#   ...allowed body...

# Authorization check using the ambient $env.CEDAR_POLICIES.
# Returns {allow: bool, decision, reasons, errors, raw}.
export def "cedar check" [
  spec: record
] {
  let entities = ($spec | get -i entities | default "[]")
  let context  = ($spec | get -i context  | default "{}")
  let schema = ($env | get -i CEDAR_SCHEMA | default "")
  let base = {
    principal: $spec.principal
    action:    $spec.action
    resource:  $spec.resource
    entities:  $entities
    context:   $context
    policies:  $env.CEDAR_POLICIES
  }
  let call = (if ($schema | is-empty) { $base } else { $base | upsert schema $schema })
  let raw = ($call | cedar authorize)
  {
    allow:    ($raw.decision == "allow")
    decision: $raw.decision
    reasons:  $raw.reasons
    errors:   $raw.errors
    raw:      $raw
  }
}

# Build a 403 response value carrying Cedar diagnostics. Returns a single
# expression so it's safe in nested closures (avoids `.response` which is
# only reliable at the top level of a handler).
export def "cedar forbidden" [
  res: record
] {
  ({error: "forbidden", reasons: $res.reasons, errors: $res.errors} | to json)
  | metadata set { merge {'http.response': {status: 403 headers: {content-type: "application/json"}}} }
}
