# CSV -> Cedar policy text. Data-driven: every classification comes from
# the CSVs themselves. Only Cedar-idiom templates are hardcoded.
#
# Inputs (4 CSVs):
#   actions.csv       (code, object_type_code, ...)
#   object_types.csv  (code, ...)
#   relations.csv     (code, object_type_code, ...)   <-- classification source
#   permissions.csv   (action_code, relation_code, event_type_code)
#
# Derivations (no hardcoded lookup tables):
#   - A relation is "platform" iff its object_type_code is "PLATFORM"
#   - PlatformRole slug for a relation: strip PLATFORM_ / ANY_ prefix, lowercase,
#     with PUBLIC -> "public" and ANY_SIGNED_IN -> "signed-in" by convention
#   - Cedar entity type for an object code: title-cased ("EVENT" -> "Event")
#   - Resource-relation attribute name: lowercase + "s" ("OWNER" -> "owners")
#
# Hardcoded (these ARE the Cedar idiom; not data):
#   PUBLIC          -> drop the when-clause
#   ANY_SIGNED_IN   -> `principal is User`
#   SELF            -> `principal == resource.user`     (the user FK convention)
#   PLATFORM rel    -> `principal in PlatformRole::"<slug>"`
#   resource rel    -> `principal in resource.<attr>`
#
# Grouping: permissions are grouped by (action_code, event_type_code) so each
# unique (action, optional-subtype) pair becomes one `permit` with the granting
# relations OR'd together. For remy-sport's 186 rows this is 99 policies.

# Title-case a single SCREAMING_SNAKE code: "EVENT" -> "Event". For multi-word
# codes (none today, but future-proof): "EVENT_TYPE" -> "EventType".
def title-case [code: string] {
  $code | split row "_" | each {|w|
    let lower = ($w | str downcase)
    # nu range 0..0 = first char only (range bounds are inclusive)
    (($lower | str substring 0..0 | str upcase)) + ($lower | str substring 1..)
  } | str join ""
}

# Derive a PlatformRole slug from the relation code. No hardcoded table.
def platform-role-slug [code: string] {
  if $code == "PUBLIC"        { return "public" }
  if $code == "ANY_SIGNED_IN" { return "signed-in" }
  if ($code | str starts-with "ANY_") { return ($code | str substring 4.. | str downcase) }
  if ($code | str starts-with "PLATFORM_") { return ($code | str substring 9.. | str downcase) }
  $code | str downcase
}

# Resource-relation attribute name. clause-for handles SELF/PLATFORM
# specially, so this is only called for resource-bound relations.
def attr-name [code: string] {
  ($code | str downcase) + "s"
}

# Build the Cedar when-clause fragment for one relation. Takes the
# pre-computed platform-relation list so the classification is data, not code.
def clause-for [
  relation_code: string
  platform_relations: list
] {
  if $relation_code == "PUBLIC"        { return "true" }
  if $relation_code == "ANY_SIGNED_IN" { return "principal is User" }
  if $relation_code == "SELF"          { return "principal == resource.user" }
  if $relation_code in $platform_relations {
    let slug = (platform-role-slug $relation_code)
    return $'principal in PlatformRole::"($slug)"'
  }
  let attr = (attr-name $relation_code)
  $"principal in resource.($attr)"
}

# Generate Cedar policy text from the four source CSVs.
#
# Returns: {text: <Cedar text>, count: <policy count>, groups: <table>}
export def "cedar codegen" [
  data: record   # {actions, object_types, relations, permissions}
] {
  # Classify relations from the data: anything attached to PLATFORM is a
  # platform relation, everything else is a resource relation.
  let platform_relations = (
    $data.relations | where object_type_code == "PLATFORM" | get code
  )

  # action_code -> object_type_code (used to pick the Cedar entity type per permit).
  let action_obj = ($data.actions | reduce -f {} {|row, acc|
    $acc | upsert $row.code $row.object_type_code
  })

  # object_type code -> Cedar entity type name, derived from object_types.csv.
  let cedar_type = ($data.object_types | reduce -f {} {|row, acc|
    $acc | upsert $row.code (title-case $row.code)
  })

  # Group permissions by (action_code, event_type_code).
  let groups = (
    $data.permissions
    | group-by --to-table { |p| $"($p.action_code)__($p.event_type_code)" }
    | each {|g|
        let first = ($g.items | first)
        let relations = ($g.items | get relation_code | uniq)
        {action: $first.action_code, event_type: $first.event_type_code, relations: $relations}
      }
  )

  # Render each group as a `permit` block.
  let policies = ($groups | enumerate | each {|enum|
    let g = $enum.item
    let i = $enum.index
    let obj_code = ($action_obj | get -i $g.action | default "PLATFORM")
    let cedar_obj = ($cedar_type | get -i $obj_code | default "Platform")

    let clauses = (
      $g.relations
      | each {|r| clause-for $r $platform_relations }
      | where {|c| $c != null }
    )

    let id_suffix = (if ($g.event_type | is-not-empty) { $"_($g.event_type)" } else { "" })
    let id = $"p($i)_($g.action)($id_suffix)"
    let header = $"@id\(\"($id)\"\)\npermit \(\n  principal,\n  action == Action::\"($g.action)\",\n  resource is ($cedar_obj)\n\)"

    if ($clauses | is-empty) {
      null
    } else if ("true" in $clauses) {
      # Unconstrained permit (PUBLIC). Drop when-block entirely.
      $"($header);"
    } else {
      # Disjunction of relations. If there's a subtype guard, wrap the
      # disjunction in parens so && binds correctly (Cedar's && is tighter than ||).
      let rels = ($clauses | str join " ||\n    ")
      let body = (if ($g.event_type | is-not-empty) {
        $"resource.type == \"($g.event_type)\" &&\n    \(($rels)\)"
      } else {
        $rels
      })
      $"($header) when {\n    ($body)\n  };"
    }
  } | where {|p| $p != null })

  {
    text:               ($policies | str join "\n\n")
    count:              ($policies | length)
    groups:             $groups
    platform_relations: $platform_relations
    cedar_types:        $cedar_type
  }
}

# Convenience: load all four source CSVs from a seed dir and codegen.
# Returns the same shape as `cedar codegen` PLUS `schema` (Cedar schema text).
export def "cedar codegen-from-dir" [
  seed_dir: path
] {
  let data = {
    actions:      (open ($seed_dir | path join "actions.csv"))
    object_types: (open ($seed_dir | path join "object_types.csv"))
    relations:    (open ($seed_dir | path join "relations.csv"))
    permissions:  (open ($seed_dir | path join "permissions.csv"))
  }
  let policies = (cedar codegen $data)
  let schema = (cedar schema $data)
  $policies | upsert schema $schema
}

# ---------------------------------------------------------------------------
# Schema generation (.cedarschema) — derived from the same CSVs.
#
# Emits:
#   - PlatformRole declaration (entity type, no attrs)
#   - User declaration (in PlatformRole, with role + name attrs)
#   - One entity per object_types.csv row, with attrs derived from
#     relations.csv (where object_type_code matches)
#   - One action per actions.csv row, with appliesTo derived from
#     actions.object_type_code -> the Cedar entity type
#
# SELF is special: emits `user: <UserType>` (singular) on Player,
# matching how codegen.nu encodes the SELF clause as `principal == resource.user`.
# Every other resource-relation emits `<attr>: Set<User>`.
# ---------------------------------------------------------------------------

export def "cedar schema" [
  data: record
] {
  let cedar_type = ($data.object_types | reduce -f {} {|row, acc|
    $acc | upsert $row.code (title-case $row.code)
  })

  # Build per-entity attribute lists from relations (skip PLATFORM relations).
  let resource_relations = ($data.relations | where object_type_code != "PLATFORM")

  let attrs_by_type = ($resource_relations
    | group-by --to-table { |r| $r.object_type_code }
    | reduce -f {} {|g, acc|
        # group-by --to-table emits {closure_0, items}; read the key off the
        # first item to avoid depending on the closure_0 column name.
        let object_type = ($g.items | first | get object_type_code)
        let attrs = ($g.items | each {|r|
          if $r.code == "SELF" {
            # Singular User attribute named "user"; matches codegen's
            # `principal == resource.user` clause.
            {name: "user", type: "User"}
          } else {
            {name: ($"($r.code | str downcase)s"), type: "Set<User>"}
          }
        })
        $acc | upsert $object_type $attrs
      })

  # Special attrs not derived from relations: Event needs `type: String` for
  # subtype guards (resource.type == "TOURNAMENT" etc.).
  let attrs_by_type_with_extras = ($attrs_by_type | upsert EVENT (
    ($attrs_by_type | get -i EVENT | default [])
    | prepend {name: "type", type: "String"}
  ))

  # Emit entity declarations.
  let entity_blocks = ($data.object_types | each {|ot|
    let ct = ($cedar_type | get $ot.code)
    let attrs = ($attrs_by_type_with_extras | get -i $ot.code | default [])
    if ($attrs | is-empty) {
      $"entity ($ct);"
    } else {
      let body = ($attrs | each {|a| $"  ($a.name): ($a.type)," } | str join "\n")
      $"entity ($ct) = {\n($body)\n};"
    }
  })

  let fixed_entities = [
    "entity PlatformRole;"
    "entity User in [PlatformRole] = {\n  role: String,\n  name: String,\n};"
  ]

  # Emit one action per actions.csv row, mapping object_type_code -> Cedar type.
  let action_blocks = ($data.actions | each {|a|
    let ct = ($cedar_type | get -i $a.object_type_code | default "Platform")
    $"action \"($a.code)\" appliesTo {\n  principal: [User],\n  resource: [($ct)],\n};"
  })

  ($fixed_entities ++ $entity_blocks ++ $action_blocks) | str join "\n\n"
}
