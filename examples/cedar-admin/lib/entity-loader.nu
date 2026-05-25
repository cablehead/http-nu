# Cedar entity loader, data-driven from relations.csv.
#
# For a given (principal, resource) pair, this module loads JUST the slice
# of entities Cedar needs to evaluate that one check:
#   - the principal User (with PlatformRole parents per users.role_code)
#   - the resource entity (Event / Team / Player / Org / Division) with
#     attrs (owners, co_organizers, head_coachs, ...) materialised from the
#     domain CSVs per each relation's derivation_kind in relations.csv
#   - the PlatformRole entities referenced by the policy
#
# Six derivation kinds, all read from relations.csv columns:
#
#   fk              source_table is the resource table itself; filter is the
#                   column holding the user_id (e.g. events.organizer_user_id)
#   junction        source_table has rows linking <resource>_id -> user_id;
#                   filter is optional `col=val` to narrow rows
#   junction_active source_table is a junction with a date column named in
#                   filter; rows are "active" when that column is empty or in
#                   the future
#   platform_role   source_table is users.csv; filter is `col=val` (typically
#                   role_code=X); ALL matching user_ids become members of
#                   the PlatformRole derived from the relation code
#   signed_in       no table; everyone with an authenticated session is in
#                   PlatformRole::"signed-in"
#   public          no table; everyone (incl. anonymous) is in
#                   PlatformRole::"public"

use ./codegen.nu *   # for `cedar codegen-from-dir` indirectly via plugin

# --- helpers ----------------------------------------------------------------

# Parse a "col=val" filter expression into {col, val} or null if empty.
def parse-eq-filter [expr: string] {
  if ($expr | is-empty) { return null }
  let parts = ($expr | split row "=" | each {|s| $s | str trim })
  if (($parts | length) != 2) { return null }
  {col: $parts.0, val: $parts.1}
}

# Map an object_type_code (EVENT/TEAM/PLAYER/ORG/DIVISION) to the column name
# used in junction tables for that resource: <lowercase>_id.
def junction-fk-col [object_type_code: string] {
  ($object_type_code | str downcase) + "_id"
}

# Map a relation code to its PlatformRole slug. Same convention codegen uses.
def role-slug-for [code: string] {
  if $code == "PUBLIC"        { return "public" }
  if $code == "ANY_SIGNED_IN" { return "signed-in" }
  if ($code | str starts-with "ANY_") { return ($code | str substring 4.. | str downcase) }
  if ($code | str starts-with "PLATFORM_") { return ($code | str substring 9.. | str downcase) }
  $code | str downcase
}

# Resource-attribute name from a relation code. Matches codegen.
def attr-name [code: string] {
  ($code | str downcase) + "s"
}

# Today's date as "YYYY-MM-DD" -- used by junction_active filtering.
def today-str [] {
  date now | format date "%Y-%m-%d"
}

# --- per-derivation-kind lookups -------------------------------------------
# Each returns a list of user_ids matching the relation for the given resource.

# fk: events.organizer_user_id -> resource row has a column holding the user_id
def lookup-fk [
  seed_dir: path
  source_table: string
  filter_col: string         # the column with the user_id
  resource_id: string
] {
  let rows = (open ($seed_dir | path join $"($source_table).csv") | where id == $resource_id)
  if ($rows | is-empty) { return [] }
  let val = ($rows | first | get $filter_col)
  if ($val | is-empty) { [] } else { [$val] }
}

# junction: event_co_organizers.user_id where event_id=<resource_id>
# fk_col_override is non-empty for polymorphic junctions (e.g. subscriptions
# uses `object_id` discriminated by `object_type_code`).
def lookup-junction [
  seed_dir: path
  source_table: string
  resource_object_type: string
  extra_filter: any           # null or {col, val}
  fk_col_override: string     # empty -> default `<object>_id`
  resource_id: string
] {
  let fk_col = (if ($fk_col_override | is-empty) {
    junction-fk-col $resource_object_type
  } else {
    $fk_col_override
  })
  let table = (open ($seed_dir | path join $"($source_table).csv"))
  let scoped = ($table | where {|r| ($r | get $fk_col) == $resource_id })
  let filtered = if $extra_filter == null { $scoped } else {
    $scoped | where {|r| ($r | get $extra_filter.col) == $extra_filter.val }
  }
  $filtered | get user_id
}

# junction_active: source_table has a user_id column directly + a date column
# named in filter (rows active when that column is empty or in the future).
def lookup-junction-active [
  seed_dir: path
  source_table: string
  resource_object_type: string
  date_col: string
  resource_id: string
] {
  let fk_col = (junction-fk-col $resource_object_type)
  let today = (today-str)
  open ($seed_dir | path join $"($source_table).csv")
  | where {|r| ($r | get $fk_col) == $resource_id }
  | where {|r|
      let d = ($r | get $date_col)
      ($d | is-empty) or ($d > $today)
    }
  | get user_id
}

# junction_active_via_players: source_table is a junction with player_id (NOT
# user_id) plus a date column for active filtering. user_id comes via a second
# hop into players.csv. Honest about real data: not every player has a user
# account, and the relation only applies when they do.
def lookup-junction-active-via-players [
  seed_dir: path
  source_table: string
  resource_object_type: string
  date_col: string
  resource_id: string
] {
  let fk_col = (junction-fk-col $resource_object_type)
  let today = (today-str)
  let player_ids = (
    open ($seed_dir | path join $"($source_table).csv")
    | where {|r| ($r | get $fk_col) == $resource_id }
    | where {|r|
        let d = ($r | get $date_col)
        ($d | is-empty) or ($d > $today)
      }
    | get player_id
  )
  if ($player_ids | is-empty) { return [] }
  open ($seed_dir | path join "players.csv")
  | where {|r| $r.id in $player_ids }
  | where {|r| not ($r.user_id | is-empty) }
  | get user_id
}

# platform_role: users where role_code=ADMIN -> all matching user ids
def lookup-platform-role [
  seed_dir: path
  source_table: string         # always "users" today
  extra_filter: record         # {col, val}, required
] {
  open ($seed_dir | path join $"($source_table).csv")
  | where {|r| ($r | get $extra_filter.col) == $extra_filter.val }
  | get id
}

# --- principal --------------------------------------------------------------

# Build the User entity for a known user_id. Parents include the platform
# role they hold (per users.role_code) plus signed-in and public.
export def load-principal [
  user_id: string
  seed_dir: path
] {
  let u = (open ($seed_dir | path join "users.csv") | where id == $user_id | get -i 0)
  if $u == null { return null }
  let role_slug = ($u.role_code | str downcase)
  {
    uid: {type: "User", id: $user_id}
    attrs: {role: $u.role_code, name: $u.name_en}
    parents: [
      {type: "PlatformRole", id: $role_slug}
      {type: "PlatformRole", id: "signed-in"}
      {type: "PlatformRole", id: "public"}
    ]
  }
}

# Anonymous principal (no session). Just in PlatformRole::"public".
export def load-anonymous []: nothing -> record {
  {
    uid: {type: "User", id: "anonymous"}
    attrs: {role: "PUBLIC", name: "Anonymous"}
    parents: [{type: "PlatformRole", id: "public"}]
  }
}

# --- resource ---------------------------------------------------------------

# Build a resource entity (Event/Team/Player/Org/Division/Platform) including
# every relation-derived attribute, by reading relations.csv as the spec and
# dispatching to the appropriate lookup-* function per derivation_kind.
export def load-resource [
  resource_type: string         # Cedar entity type name: Event/Team/Player/Org/Division/Platform
  resource_id: string
  seed_dir: path
] {
  if $resource_type == "Platform" {
    return {uid: {type: "Platform", id: $resource_id}, attrs: {}, parents: []}
  }

  let object_type_code = ($resource_type | str upcase)   # Event -> EVENT
  let relations = (
    open ($seed_dir | path join "relations.csv")
    | where object_type_code == $object_type_code
  )

  # Load the resource's own row first (gives us its `type_code` etc. for Events).
  let table_name = ($resource_type | str downcase) + "s"   # Event -> events
  let row = (open ($seed_dir | path join $"($table_name).csv") | where id == $resource_id | get -i 0)
  if $row == null { return null }

  # For Events, expose `type` (TOURNAMENT/LEAGUE/CAMP/SHOWCASE) for subtype guards.
  mut attrs = if $resource_type == "Event" { {type: $row.type_code} } else { {} }

  # For each relation on this object type, materialise the attribute list.
  # SELF is special: emits a SINGLE User attribute named `user` (matches
  # codegen's `principal == resource.user` clause + schema's `user: User`).
  # All other relations emit a Set<User> at `<code lowercase>s`.
  for rel in $relations {
    let extra = (parse-eq-filter $rel.filter)
    let user_ids = (match $rel.derivation_kind {
      "fk" => (lookup-fk $seed_dir $rel.source_table $rel.filter $resource_id)
      "junction" => (lookup-junction $seed_dir $rel.source_table $object_type_code $extra ($rel.fk_column? | default "") $resource_id)
      "junction_active" => (lookup-junction-active $seed_dir $rel.source_table $object_type_code $rel.filter $resource_id)
      "junction_active_via_players" => (lookup-junction-active-via-players $seed_dir $rel.source_table $object_type_code $rel.filter $resource_id)
      _ => []   # platform/signed_in/public don't apply to resources
    })
    if $rel.code == "SELF" {
      # Single User (not Set). Skip if no user_id resolved.
      if (not ($user_ids | is-empty)) {
        $attrs = ($attrs | upsert "user" {__entity: {type: "User", id: ($user_ids | first)}})
      }
    } else {
      let attr = (attr-name $rel.code)
      let entity_refs = ($user_ids | each {|uid| {__entity: {type: "User", id: $uid}}})
      $attrs = ($attrs | upsert $attr $entity_refs)
    }
  }

  {uid: {type: $resource_type, id: $resource_id}, attrs: $attrs, parents: []}
}

# --- platform-role entities -------------------------------------------------

# Build the PlatformRole entities that policies reference. One per role
# derived from relations.csv (object_type_code=PLATFORM). Membership is NOT
# encoded as parents on PlatformRole; principals get role parents in
# load-principal. PlatformRole entities exist so Cedar's `principal in
# PlatformRole::"admin"` resolves the entity uid.
export def load-platform-roles [
  seed_dir: path
] {
  open ($seed_dir | path join "relations.csv")
  | where object_type_code == "PLATFORM"
  | get code
  | each {|c| role-slug-for $c }
  | uniq
  | each {|slug| {uid: {type: "PlatformRole", id: $slug}, attrs: {}, parents: []} }
}

# --- the slice loader -------------------------------------------------------

# Top-level: given principal + resource, return the full Cedar entities JSON
# for the check.
export def load-slice [
  principal_id: any                 # string user_id or null for anonymous
  resource_type: string             # Cedar entity type name
  resource_id: string
  seed_dir: path
] {
  let principal = (if $principal_id == null {
    load-anonymous
  } else {
    load-principal $principal_id $seed_dir
  })
  if $principal == null {
    error make {msg: $"unknown principal user_id: ($principal_id)"}
  }

  let resource = (load-resource $resource_type $resource_id $seed_dir)
  let platform_roles = (load-platform-roles $seed_dir)

  let entities = ([$principal] ++ $platform_roles)
  if $resource != null { $entities ++ [$resource] } else { $entities }
}
