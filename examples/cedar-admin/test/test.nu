# Pure unit tests for cedar-admin. Mirrors examples/2048/test/test.nu
# in shape: `use std/assert`, paths resolved from `path self`, tests
# group by module. Runs via:
#   http-nu --plugin target/debug/nu_plugin_cedar eval examples/cedar-admin/test/test.nu

use std/assert
use http-nu/cedar *

const script_dir = path self | path dirname
const example_dir = $script_dir | path dirname
const seed_dir = $example_dir | path join "seed"

use ($example_dir | path join "lib" "codegen.nu") *
use ($example_dir | path join "lib" "entity-loader.nu") *
use ($example_dir | path join "auth.nu") *

# ----------------------------------------------------------------------------
# codegen.nu -- data-driven classification + policy generation
# ----------------------------------------------------------------------------

let cg = (cedar codegen-from-dir $seed_dir)

# 186 original permission rows + 4 self-protection (VIEW_ACCESS_MATRIX,
# VIEW_OWN_PERMISSIONS, EDIT_POLICY, DELETE_POLICY) = 190 raw, 103 grouped.
assert ($cg.count == 103) $"expected 103 policies, got ($cg.count)"
assert (($cg.text | str length) > 1000) "generated text should be substantial"
assert (($cg.schema | str length) > 1000) "generated schema should be substantial"
assert ($cg.schema | str contains "entity User in [PlatformRole]") "schema declares User in PlatformRole"
assert ($cg.schema | str contains "entity Event = {") "schema declares Event entity"
assert ($cg.schema | str contains "entity Player = {") "schema declares Player entity"
assert ($cg.schema | str contains "user: User,") "Player declares singular `user` attr (for SELF)"
assert ($cg.schema | str contains "owners: Set<User>,") "Event declares owners as Set<User>"
assert ($cg.schema | str contains "action \"EDIT_EVENT\" appliesTo") "actions declare appliesTo"
assert ($cg.schema | str contains "action \"EDIT_POLICY\" appliesTo") "EDIT_POLICY action declared (self-protection)"
assert ($cg.schema | str contains "action \"DELETE_POLICY\" appliesTo") "DELETE_POLICY action declared"
assert ($cg.text | str contains "EDIT_POLICY") "EDIT_POLICY permit emitted"

# Platform relations must be DERIVED from relations.csv (object_type_code=PLATFORM),
# not hardcoded. The 8 expected codes appear in seed/relations.csv.
let expected_platform = ["PLATFORM_ADMIN" "ANY_ORGANIZER" "ANY_COACH" "ANY_PLAYER" "ANY_REFEREE" "ANY_SPECTATOR" "ANY_SIGNED_IN" "PUBLIC"]
for code in $expected_platform {
  assert ($code in $cg.platform_relations) $"expected ($code) in derived platform_relations"
}

# Cedar entity types are derived from object_types.csv by title-casing.
assert (($cg.cedar_types | get EVENT) == "Event") "EVENT -> Event"
assert (($cg.cedar_types | get TEAM)  == "Team")  "TEAM -> Team"
assert (($cg.cedar_types | get PLAYER) == "Player") "PLAYER -> Player"
assert (($cg.cedar_types | get PLATFORM) == "Platform") "PLATFORM -> Platform"

# Subtype-guarded policies must wrap the relations disjunction in parens
# (regression: Cedar's && is tighter than ||; without parens the guard
# would apply only to the last clause).
let assign_courts_league = ($cg.text | str index-of "p41_ASSIGN_COURTS_LEAGUE")
assert ($assign_courts_league >= 0) "ASSIGN_COURTS_LEAGUE policy must exist"
let fragment = ($cg.text | str substring ($assign_courts_league - 1)..($assign_courts_league + 400))
assert ($fragment | str contains "resource.type == \"LEAGUE\" &&") "subtype guard precedes the disjunction"
assert ($fragment | str contains "(principal in resource.owners") "disjunction wrapped in parens"

# Plugin must parse BOTH the policies AND the schema without error. Setting
# CEDAR_SCHEMA validates entity shapes + request types on every check.
$env.CEDAR_POLICIES = $cg.text
$env.CEDAR_SCHEMA   = $cg.schema

# Minimal entity slice for schema-validated request. BROWSE_EVENTS is a
# Platform-level action per actions.csv (object_type_code=PLATFORM), so the
# resource must be Platform. The schema enforces this.
let smoke_entities = [
  { uid: {type: "User", id: "any"}, attrs: {role: "ANON", name: "anon"}, parents: [] }
  { uid: {type: "Platform", id: "global"}, attrs: {}, parents: [] }
]
let r = (cedar check {
  principal: 'User::"any"'
  action:    'Action::"BROWSE_EVENTS"'
  resource:  'Platform::"global"'
  entities:  ($smoke_entities | to json -r)
})
assert ($r.decision == "allow") $"BROWSE_EVENTS by anyone should allow; got ($r.decision) reasons=($r.reasons) errors=($r.errors)"
assert (($r.errors | length) == 0) $"no errors expected; got ($r.errors)"

# ----------------------------------------------------------------------------
# auth.nu -- identity lookup from seed/users.csv + roles.csv + user_statuses.csv
# ----------------------------------------------------------------------------

# 12 users total in seed; one is non-ACTIVE so the login list shows 11.
let login_users = (list-users-for-login $seed_dir)
assert (($login_users | length) == 11) $"expected 11 active users in login list, got ($login_users | length)"

# All six role codes must appear (matches seed/roles.csv).
let roles_seen = ($login_users | get role_code | uniq | sort)
let expected_roles = ["ADMIN" "COACH" "ORGANIZER" "PLAYER" "REFEREE" "SPECTATOR"]
assert ($roles_seen == $expected_roles) $"expected roles ($expected_roles), got ($roles_seen)"

# Admin detection is data-driven (reads users.role_code == ADMIN).
assert ((is-admin "usr_admin_001" $seed_dir) == true) "usr_admin_001 must be admin"
assert ((is-admin "usr_org_001"   $seed_dir) == false) "usr_org_001 must NOT be admin"
assert ((is-admin "usr_does_not_exist" $seed_dir) == false) "ghost user must NOT be admin"

# Active-status check.
assert ((is-active "usr_admin_001" $seed_dir) == true) "usr_admin_001 must be active"
assert ((is-active "usr_does_not_exist" $seed_dir) == false) "ghost user must NOT be active"

# Lookup returns the row record or null.
let admin = (lookup-user "usr_admin_001" $seed_dir)
assert ($admin != null) "lookup-user should return record for known user"
assert ($admin.role_code == "ADMIN") "admin record has role_code=ADMIN"

let ghost = (lookup-user "usr_does_not_exist" $seed_dir)
assert ($ghost == null) "lookup-user should return null for unknown user"

# ----------------------------------------------------------------------------
# entity-loader.nu -- data-driven from relations.csv
# ----------------------------------------------------------------------------

# Principal: usr_admin_001 must come out with PlatformRole::admin + signed-in + public parents.
let alice = (load-principal "usr_admin_001" $seed_dir)
assert ($alice.uid.id == "usr_admin_001") "principal id"
let parent_ids = ($alice.parents | get id | sort)
assert ($parent_ids == ["admin" "public" "signed-in"]) $"admin parents: got ($parent_ids)"

# Anonymous principal: only public, id="anonymous".
let anon = (load-anonymous)
assert ($anon.uid.id == "anonymous") "anon id"
assert (($anon.parents | get id) == ["public"]) "anon parents"

# Platform roles entities: must include admin + the 5 role slugs + signed-in + public.
let roles = (load-platform-roles $seed_dir)
let role_ids = ($roles | get uid.id | sort)
let expected_role_ids = ["admin" "coach" "organizer" "player" "public" "referee" "signed-in" "spectator"]
assert ($role_ids == $expected_role_ids) $"PlatformRole ids: got ($role_ids)"

# Resource: Event evt_001 -- alice org_001 is the owner (events.organizer_user_id=usr_org_001),
# bob is the co-organizer (event_co_organizers row).
let evt = (load-resource "Event" "evt_001" $seed_dir)
assert ($evt.uid.id == "evt_001") "event id"
assert ($evt.attrs.type == "TOURNAMENT") "event subtype carried through"
let owners = ($evt.attrs.owners | get __entity.id)
assert ($owners == ["usr_org_001"]) $"OWNER fk lookup: got ($owners)"
let co_orgs = ($evt.attrs.co_organizers | get __entity.id)
assert ($co_orgs == ["usr_org_002"]) $"CO_ORGANIZER junction lookup: got ($co_orgs)"

# Resource: Team team_001 -- exercises the `junction with filter` derivation kind.
# team_coaches has rows for team_001 with HEAD / ASSISTANT.
let team = (load-resource "Team" "team_001" $seed_dir)
let heads = ($team.attrs.head_coachs | get __entity.id)
let assistants = ($team.attrs.assistant_coachs | get __entity.id)
assert ($heads == ["usr_coach_001"]) $"HEAD_COACH filtered junction: got ($heads)"
assert ($assistants == ["usr_coach_002"]) $"ASSISTANT_COACH filtered junction: got ($assistants)"

# Slice: putting it together -- a real check against the generated policies.
$env.CEDAR_POLICIES = $cg.text
let slice_admin = (load-slice "usr_admin_001" "Event" "evt_001" $seed_dir)
let check_admin = (cedar check {
  principal: 'User::"usr_admin_001"'
  action:    'Action::"DELETE_EVENT"'
  resource:  'Event::"evt_001"'
  entities:  ($slice_admin | to json -r)
})
assert ($check_admin.decision == "allow") $"DELETE_EVENT by admin should allow; got ($check_admin.decision) reasons=($check_admin.reasons)"

# Non-admin, non-owner trying to DELETE_EVENT should be denied.
let slice_player = (load-slice "usr_player_001" "Event" "evt_001" $seed_dir)
let check_player = (cedar check {
  principal: 'User::"usr_player_001"'
  action:    'Action::"DELETE_EVENT"'
  resource:  'Event::"evt_001"'
  entities:  ($slice_player | to json -r)
})
assert ($check_player.decision == "deny") $"DELETE_EVENT by player should deny; got ($check_player.decision)"

# The actual event owner (usr_org_001) should be allowed to EDIT_EVENT.
let slice_owner = (load-slice "usr_org_001" "Event" "evt_001" $seed_dir)
let check_owner = (cedar check {
  principal: 'User::"usr_org_001"'
  action:    'Action::"EDIT_EVENT"'
  resource:  'Event::"evt_001"'
  entities:  ($slice_owner | to json -r)
})
assert ($check_owner.decision == "allow") $"EDIT_EVENT by owner should allow; got ($check_owner.decision) reasons=($check_owner.reasons)"

print "ok"
