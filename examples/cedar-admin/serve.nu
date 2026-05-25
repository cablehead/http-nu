# cedar-admin — live editor for remy-sport's access-policy CSVs, gated by
# Cedar policies generated from those same CSVs.
#
# Run:
#   mise run ex:cedar-admin
# Or directly:
#   ./target/debug/http-nu \
#     --plugin ./target/debug/nu_plugin_cedar \
#     --datastar --store ./.store \
#     :3001 examples/cedar-admin/serve.nu

use http-nu/html *
use http-nu/cedar *
use http-nu/http *
use http-nu/datastar *
use ./auth.nu *
use ./lib/codegen.nu *
use ./lib/entity-loader.nu *
use ./lib/views.nu *

const SCRIPT_DIR = path self | path dirname
const SEED_DIR   = $SCRIPT_DIR | path join "seed"
const STATIC_DIR = $SCRIPT_DIR | path join "static"

# Boot announce. Per-request codegen below sees live CSV edits without restart.
# The SSE projection on /policies/permissions/sse pushes patches as
# `cedar.policy.edited` frames land on xs.
let _BOOT = (cedar codegen-from-dir $SEED_DIR)
print $"cedar-admin: started — ($_BOOT.count) policies, ($_BOOT.groups | length) groups"

# Per-server-restart cache-buster for static assets. Browsers cache
# styles.css?v=<REV> across requests but refetch when the server restarts
# (REV changes). 2048's serve.nu uses the same pattern.
$env.STATIC_REV = (random uuid | str substring 0..7)

const STATIC_PREFIX_LEN = 8   # "/static/"

# Run a Cedar check using the current (just-regenerated) policies + schema.
# Returns the auth record from `cedar check`.
def cedar-gate [
  cg: record
  user: any                          # null for anonymous
  action: string                     # action code, e.g. "EDIT_POLICY"
  resource_type: string              # Cedar entity type, e.g. "Platform"
  resource_id: string                # entity id, e.g. "global"
] {
  $env.CEDAR_POLICIES = $cg.text
  $env.CEDAR_SCHEMA   = $cg.schema
  let principal_id = (if $user == null { null } else { $user.id })
  let entities = (load-slice $principal_id $resource_type $resource_id $SEED_DIR)
  let principal_str = (if $user == null { 'User::"anonymous"' } else { $"User::\"($user.id)\"" })
  cedar check {
    principal: $principal_str
    action:    $"Action::\"($action)\""
    resource:  $"($resource_type)::\"($resource_id)\""
    entities:  ($entities | to json -r)
  }
}

# Append an audit-log frame to xs. Stamped with by-user + RFC3339 timestamp.
def audit-policy-edit [
  op: string         # "add" | "remove"
  csv: string        # "permissions" | "actions" | "relations" | "object_types" | "event_types"
  row: record
  user: any
] {
  let by = (if $user == null { "anonymous" } else { $user.id })
  null | .append "cedar.policy.edited" --meta {
    op: $op
    csv: $csv
    row: $row
    by: $by
    at: (date now | format date "%Y-%m-%dT%H:%M:%S%:z")
  }
}

# The 5 CSVs that drive Cedar codegen. Edits to any of them go through
# EDIT_POLICY / DELETE_POLICY gates and trigger the same SSE projection.
const POLICY_CSVS = ["permissions", "actions", "relations", "object_types", "event_types"]

# Generic GET handler for /policies/<name>. Cedar checks twice -- once for
# VIEW gate (informational), once for EDIT to decide button visibility.
def handle-policy-get [
  name: string
  user: any
  cg: record
  seed_dir: path
] {
  let _view = (cedar-gate $cg $user "VIEW_OWN_PERMISSIONS" "Platform" "global")
  let can_edit = (((cedar-gate $cg $user "EDIT_POLICY" "Platform" "global") | get allow))
  let rows = (open ($seed_dir | path join $"($name).csv"))
  page-policy-editor $name $rows $user $can_edit
}

# Generic SSE handler for /policies/<name>/sse. Emits the current tbody on
# connect, then re-renders + patches on each cedar.policy.edited frame whose
# `csv` meta matches this name (so editors don't see each other's noise).
def handle-policy-sse [
  name: string
  user: any
  cg: record
  seed_dir: path
] {
  let can_edit = (((cedar-gate $cg $user "EDIT_POLICY" "Platform" "global") | get allow))
  let selector = $"#($name)-tbody"

  null | interleave { ||
    let rows = (open ($seed_dir | path join $"($name).csv"))
    policy-tbody $name $rows $can_edit
    | to datastar-patch-elements --selector $selector --mode "outer"
  } { ||
    .cat -T cedar.policy.edited --follow --new
    | where {|frame| ($frame.meta.csv? | default "") == $name }
    | each {|frame|
        let rows = (open ($seed_dir | path join $"($name).csv"))
        policy-tbody $name $rows $can_edit
        | to datastar-patch-elements --selector $selector --mode "outer"
      }
  } | to sse
}

# Generic POST handler for /policies/<name>. Body is form-encoded with one
# field per CSV column; unspecified fields become empty strings.
def handle-policy-post-add [
  name: string
  user: any
  body: any
  cg: record
  seed_dir: path
] {
  let auth = (cedar-gate $cg $user "EDIT_POLICY" "Platform" "global")
  if not $auth.allow {
    cedar forbidden $auth
  } else {
    let form = ($body | from url)
    let file = ($seed_dir | path join $"($name).csv")
    let existing = (open $file)
    let cols = (if ($existing | is-empty) { [] } else { $existing | first | columns })
    let row = ($cols | reduce -f {} {|c, acc|
      $acc | upsert $c (($form | get -i $c | default "") | str trim)
    })
    # Validate: at least one non-empty value required.
    let any_filled = ($cols | any {|c| (($row | get -i $c | default "") | is-not-empty) })
    if not $any_filled {
      "at least one field is required" | metadata set { merge {'http.response': {status: 400}} }
    } else {
      let new_table = ($existing | append $row)
      $new_table | save -f $file
      audit-policy-edit "add" $name $row $user | ignore
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $"/policies/($name)"}}} }
    }
  }
}

def handle-policy-post-delete [
  name: string
  user: any
  body: any
  cg: record
  seed_dir: path
] {
  let auth = (cedar-gate $cg $user "DELETE_POLICY" "Platform" "global")
  if not $auth.allow {
    cedar forbidden $auth
  } else {
    let form = ($body | from url)
    let row_idx = ($form | get row? | default "" | into int)
    let file = ($seed_dir | path join $"($name).csv")
    let existing = (open $file)
    if $row_idx < 0 or $row_idx >= ($existing | length) {
      "row index out of range" | metadata set { merge {'http.response': {status: 400}} }
    } else {
      let removed = ($existing | get $row_idx)
      let new_table = ($existing | drop nth $row_idx)
      $new_table | save -f $file
      audit-policy-edit "remove" $name $removed $user | ignore
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $"/policies/($name)"}}} }
    }
  }
}

{|req|
  let body = $in

  # Per-request codegen so edits take immediate effect on the very next check.
  # Tier 2 swaps this for an xs-projected state.
  let CG = (cedar codegen-from-dir $SEED_DIR)

  # Resolve the active session (null if anonymous).
  let session = (resolve-session $req)
  let user = (if $session == null { null } else {
    lookup-user $session.user_id $SEED_DIR
  })

  match $req {
    {method: "GET", path: "/"} => {
      page-home $user
    }

    {method: "GET", path: "/login"} => {
      page-login (list-users-for-login $SEED_DIR)
    }

    {method: "POST", path: "/login"} => {
      let form = ($body | from url)
      let user_id = ($form | get user_id? | default "")
      let target = (lookup-user $user_id $SEED_DIR)
      if $target == null {
        $"Unknown user_id: ($user_id)" | metadata set { merge {'http.response': {status: 400}} }
      } else if $target.status_code != "ACTIVE" {
        $"Account ($user_id) is not active — status=($target.status_code)" | metadata set { merge {'http.response': {status: 403}} }
      } else {
        let new_session = (mint-session $user_id)
        "" | metadata set { merge {'http.response': {status: 302 headers: {Location: "/me"}}} }
        | session-cookies set $new_session
      }
    }

    {method: "POST", path: "/logout"} => {
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: "/"}}} }
      | session-cookies clear
    }

    {method: "GET", path: "/me"} => {
      page-me $user
    }

    {method: "GET", path: "/matrix"} => {
      let actions      = (open ($SEED_DIR | path join "actions.csv"))
      let permissions  = (open ($SEED_DIR | path join "permissions.csv"))
      let object_types = (open ($SEED_DIR | path join "object_types.csv"))
      page-matrix $CG $actions $permissions $object_types
    }

    # Cedar check playground. Renders a form for picking
    # (principal, action, resource_type, resource_id) and (on POST) runs
    # cedar check + shows decision + reasons + errors. Read-only diagnostic --
    # NOT gated by Cedar itself, anonymous can use it. Lets you exercise the
    # full entity loader (OWNER, HEAD_COACH, GUARDIAN, ...) which the editor
    # self-protection alone doesn't touch.
    # Generic CSV browser. /data lists every CSV in seed/; /data/<name>
    # renders that CSV as a table. Read-only for now; the policies CSV
    # editor at /policies/permissions is the only edit surface.
    {method: "GET", path: "/data"} => {
      let entries = (
        glob ($SEED_DIR | path join "*.csv") | each {|f|
          let n = ($f | path basename | str replace ".csv" "")
          let count = (try { (open $f | length) } catch { 0 })
          {name: $n, rows: $count}
        } | sort-by name
      )
      page-data-index $entries
    }

    {method: "GET", path: "/check"} => {
      let users        = (open ($SEED_DIR | path join "users.csv"))
      let actions      = (open ($SEED_DIR | path join "actions.csv"))
      let object_types = (open ($SEED_DIR | path join "object_types.csv"))
      page-check $users $actions $object_types {} null
    }

    {method: "POST", path: "/check"} => {
      let form = ($body | from url)
      let users        = (open ($SEED_DIR | path join "users.csv"))
      let actions      = (open ($SEED_DIR | path join "actions.csv"))
      let object_types = (open ($SEED_DIR | path join "object_types.csv"))

      let principal_id = ($form | get principal? | default "")
      let action       = ($form | get action?    | default "")
      let resource_type = ($form | get resource_type? | default "Platform")
      let resource_id   = ($form | get resource_id?   | default "global")

      let principal_user = (if ($principal_id | is-empty) { null } else { $principal_id })
      let auth = (cedar-gate $CG (if $principal_user == null { null } else { (lookup-user $principal_user $SEED_DIR) }) $action $resource_type $resource_id)
      let result = {decision: $auth.decision, reasons: $auth.reasons, errors: $auth.errors}
      page-check $users $actions $object_types $form $result
    }

    # Generic policy-CSV editor. Same 4 routes for each of the 5 CSVs that
    # drive Cedar codegen. The page-policy-editor and policy-tbody view
    # helpers handle column variance: form fields and table columns come
    # from the CSV's own headers.
    {method: "GET", path: "/policies/permissions"}     => (handle-policy-get  "permissions"  $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/actions"}         => (handle-policy-get  "actions"      $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/relations"}       => (handle-policy-get  "relations"    $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/object_types"}    => (handle-policy-get  "object_types" $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/event_types"}     => (handle-policy-get  "event_types"  $user $CG $SEED_DIR)

    {method: "GET", path: "/policies/permissions/sse"}  => (handle-policy-sse "permissions"  $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/actions/sse"}      => (handle-policy-sse "actions"      $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/relations/sse"}    => (handle-policy-sse "relations"    $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/object_types/sse"} => (handle-policy-sse "object_types" $user $CG $SEED_DIR)
    {method: "GET", path: "/policies/event_types/sse"}  => (handle-policy-sse "event_types"  $user $CG $SEED_DIR)

    {method: "POST", path: "/policies/permissions"}  => (handle-policy-post-add "permissions"  $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/actions"}      => (handle-policy-post-add "actions"      $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/relations"}    => (handle-policy-post-add "relations"    $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/object_types"} => (handle-policy-post-add "object_types" $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/event_types"}  => (handle-policy-post-add "event_types"  $user $body $CG $SEED_DIR)

    {method: "POST", path: "/policies/permissions/delete"}  => (handle-policy-post-delete "permissions"  $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/actions/delete"}      => (handle-policy-post-delete "actions"      $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/relations/delete"}    => (handle-policy-post-delete "relations"    $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/object_types/delete"} => (handle-policy-post-delete "object_types" $user $body $CG $SEED_DIR)
    {method: "POST", path: "/policies/event_types/delete"}  => (handle-policy-post-delete "event_types"  $user $body $CG $SEED_DIR)

    _ => {
      if ($req.path | str starts-with "/static/") {
        let rel = ($req.path | str substring $STATIC_PREFIX_LEN..)
        .static $STATIC_DIR $rel
      } else if ($req.path | str starts-with "/data/") and $req.method == "GET" {
        # /data/<name> -- render that seed CSV as a generic table.
        # Reject path traversal + force .csv suffix; only files in seed/ are
        # reachable, and only with simple [a-z0-9_] names.
        let name = ($req.path | str substring 6..)
        let safe = ($name =~ '^[a-z][a-z0-9_]*$')
        let file = ($SEED_DIR | path join $"($name).csv")
        if (not $safe) or (not ($file | path exists)) {
          page-not-found $req | metadata set { merge {'http.response': {status: 404}} }
        } else {
          let rows = (open $file)
          page-data-table $name $rows
        }
      } else {
        page-not-found $req | metadata set { merge {'http.response': {status: 404}} }
      }
    }
  }
}
