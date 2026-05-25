# Page renderers. Returns http-nu/html DSL records.
#
# Shape borrowed from examples/2048/tfe/render.nu: one `layout` helper for the
# shared shell, then one `page-*` per route. All HTML built via the http-nu/html
# DSL -- never as raw strings.

use http-nu/html *
use http-nu/datastar *

# Shared page shell. `body` is a list of HTML records placed inside <main>.
# Outer parens around the whole HTML call span multiple lines -- without them
# nu parses HEAD and BODY as separate statements and only the last one survives.
export def layout [title: string body: list] {
  # Per-server-restart cache-buster on static assets. STATIC_REV is set in
  # serve.nu's bootstrap; falls back to "dev" so direct eval (no server) works.
  let rev = ($env | get -i STATIC_REV | default "dev")
  (HTML
    (HEAD
      (META {charset: "UTF-8"})
      (META {name: "viewport" content: "width=device-width, initial-scale=1"})
      (TITLE $"($title) — cedar-admin")
      (LINK {rel: "stylesheet" href: $"/static/styles.css?v=($rev)"})
      (SCRIPT-DATASTAR))
    (BODY
      (HEADER
        (NAV {class: "site-nav"}
          (A {href: "/" class: "brand"} "cedar-admin")
          (DIV {class: "nav-links"}
            (A {href: "/matrix"} "Matrix")
            (A {href: "/policies/permissions"} "Permissions")
            (A {href: "/data"} "Data")
            (A {href: "/check"} "Check")
            (A {href: "/me"} "Me"))))
      (MAIN ...$body)))
}

export def page-home [user: any] {
  layout "Home" [
    (H1 "cedar-admin")
    (P "Live editor for the remy-sport access-policy CSVs, gated by the policies themselves.")
    (if $user == null {
      (DIV {class: "card"}
        (P "Not signed in.")
        (A {href: "/login" class: "btn"} "Sign in"))
    } else {
      (DIV {class: "card"}
        (P $"Signed in as ($user.name_en) [($user.role_code)]")
        (FORM {method: "POST" action: "/logout"}
          (BUTTON {type: "submit" class: "btn"} "Sign out")))
    })
  ]
}

export def page-login [active_users: list] {
  layout "Sign in" [
    (H1 "Sign in")
    (P "Pick a user to sign in as. Demo only — no password.")
    (FORM {method: "POST" action: "/login" class: "login-form"}
      (LABEL "User"
        (SELECT {name: "user_id" required: "required"}
          ...($active_users | each {|u|
            OPTION {value: $u.id} $"($u.name_en) — ($u.role_code)"
          })))
      (BUTTON {type: "submit" class: "btn"} "Sign in"))
  ]
}

export def page-me [user: any] {
  layout "Me" [
    (H1 "Current session")
    (if $user == null {
      (DIV {class: "card"}
        (P "Not signed in.")
        (A {href: "/login" class: "btn"} "Sign in"))
    } else {
      (DIV {class: "card"}
        (DL
          (DT "User ID") (DD $user.id)
          (DT "Name")    (DD $user.name_en)
          (DT "Role")    (DD $user.role_code)
          (DT "Status")  (DD $user.status_code))
        (FORM {method: "POST" action: "/logout"}
          (BUTTON {type: "submit" class: "btn"} "Sign out")))
    })
  ]
}

# Subtype letter abbreviations matching remy-sport-biz/access/matrix.md
# convention: T=Tournament, L=League, K=Camp, Sh=Showcase.
def event-subtype-letter [code: string] {
  match $code {
    "TOURNAMENT" => "T"
    "LEAGUE"     => "L"
    "CAMP"       => "K"
    "SHOWCASE"   => "Sh"
    _            => $code
  }
}

# Render the "granted to" cell for one action. Permissions for the same
# action+relation pair may appear multiple times with different event_type;
# fold them: `OWNER` (unscoped) or `OWNER (T,L,Sh)` (scoped).
def render-grants [perms_for_action: list] {
  $perms_for_action
  | group-by --to-table { |p| $p.relation_code }
  | each {|g|
      let rel = ($g.items | first | get relation_code)
      let subtypes = ($g.items | get event_type_code | where {|s| ($s | is-not-empty) })
      if ($subtypes | is-empty) {
        CODE $rel
      } else {
        let letters = ($subtypes | each {|s| event-subtype-letter $s } | str join ",")
        SPAN (CODE $rel) $" \(($letters)\)"
      }
    }
  | reduce -f [] {|cell, acc|
      if ($acc | is-empty) { [$cell] } else { $acc | append [(SPAN ", ") $cell] | flatten }
    }
}

# One section per object_type. Within: a table listing each action + its
# granted-to relations. Mirrors remy-sport-biz/access/matrix.md.
export def page-matrix [
  cg: record
  actions: list           # rows from actions.csv
  permissions: list       # rows from permissions.csv
  object_types: list      # rows from object_types.csv
] {
  let sections = ($object_types | each {|ot|
    let acts = ($actions | where object_type_code == $ot.code)
    if ($acts | is-empty) { null } else {
      let rows = ($acts | each {|a|
        let perms = ($permissions | where action_code == $a.code)
        let grants_cells = (render-grants $perms)
        (TR
          (TD (CODE $a.code))
          (TD $a.name_en)
          (TD ...$grants_cells))
      })
      (SECTION
        (H2 $"($ot.name_en) actions")
        (P {class: "muted"} $ot.description_en)
        (TABLE {class: "data-table"}
          (THEAD (TR (TH "Action") (TH "Name") (TH "Granted to")))
          (TBODY ...$rows)))
    }
  } | where {|s| $s != null })

  layout "Access Matrix" [
    (H1 "Access Matrix")
    (P $"($actions | length) actions across ($object_types | length) object types, with ($permissions | length) permission rows generating ($cg.count) Cedar policies.")
    (P {class: "muted"} "Subtypes: T=Tournament, L=League, K=Camp, Sh=Showcase. An unscoped relation grants the action for all subtypes of that object type.")
    ...$sections
    (DETAILS
      (SUMMARY "Generated policies.cedar (raw text)")
      (PRE $cg.text))
    (DETAILS
      (SUMMARY "Generated policies.cedarschema (raw text)")
      (PRE $cg.schema))
  ]
}

# --- generic policy-CSV editor -------------------------------------------
# One pair of helpers (tbody + page) drives editors for permissions, actions,
# relations, object_types, event_types. The shape is uniform: each CSV is
# rendered as a table with one column per CSV column + an optional delete
# column. Edit gates on EDIT_POLICY / DELETE_POLICY (read from `cg` via the
# caller); rows come from `open seed/<name>.csv`.

# Render the <tbody> for a generic policy CSV. The SSE handler reuses this.
export def policy-tbody [
  name: string         # csv basename, e.g. "permissions"
  rows: list
  can_edit: bool
] {
  let cols = (if ($rows | is-empty) { [] } else { $rows | first | columns })
  (TBODY {id: $"($name)-tbody"} ...($rows | enumerate | each {|enum|
    let r = $enum.item
    let i = $enum.index
    let data_cells = ($cols | each {|c|
      let v = ($r | get -i $c | default "")
      if ($v | is-empty) {
        (TD (SPAN {class: "muted"} "—"))
      } else {
        (TD (CODE $v))
      }
    })
    let delete_cell = (if not $can_edit {
      (TD {style: "display:none"})
    } else {
      (TD (FORM {method: "POST" action: $"/policies/($name)/delete" class: "inline-form"}
        (INPUT {type: "hidden" name: "row" value: ($i | into string)})
        (BUTTON {type: "submit" class: "btn-link"} "delete")))
    })
    (TR ...($data_cells | append $delete_cell))
  }))
}

export def page-policy-editor [
  name: string         # csv basename, e.g. "permissions"
  rows: list
  user: any
  can_edit: bool       # EDIT_POLICY check result
] {
  let cols = (if ($rows | is-empty) { [] } else { $rows | first | columns })
  let signed_in_note = (if $user == null {
    (P {class: "muted"} (A {href: "/login"} "Sign in") " as an admin to add or delete rows.")
  } else if not $can_edit {
    (P {class: "muted"} $"Signed in as ($user.name_en) [($user.role_code)] — not a PLATFORM_ADMIN, edit disabled.")
  } else {
    (P {class: "muted"} $"Signed in as ($user.name_en) — PLATFORM_ADMIN. Edit enabled.")
  })

  let add_form = (if (not $can_edit) or ($cols | is-empty) { (DIV) } else {
    (FORM {method: "POST" action: $"/policies/($name)" class: "row-form"}
      ...($cols | each {|c|
        (LABEL $c (INPUT {type: "text" name: $c placeholder: $"new ($c)"}))
      })
      (BUTTON {type: "submit" class: "btn"} "Add row"))
  })

  layout $"Policies: ($name)" [
    (NAV {class: "muted"}
      (A {href: "/policies/permissions"} "permissions") " · "
      (A {href: "/policies/actions"} "actions") " · "
      (A {href: "/policies/relations"} "relations") " · "
      (A {href: "/policies/object_types"} "object_types") " · "
      (A {href: "/policies/event_types"} "event_types"))
    (H1 $"($name).csv")
    (P {class: "muted"} $"($rows | length) rows, ($cols | length) columns")
    $signed_in_note
    $add_form
    (TABLE {class: "data-table"
            "data-init": $"@get\('/policies/($name)/sse', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity}\)"}
      (THEAD (TR ...($cols | each {|c| TH $c }) (if $can_edit { TH "" } else { TH {style: "display:none"} ""})))
      (policy-tbody $name $rows $can_edit))
  ]
}

# Browse one CSV as a generic table. Used by /data/<name>.
export def page-data-table [
  name: string         # csv basename, e.g. "events"
  rows: list           # parsed rows
] {
  let cols = (if ($rows | is-empty) { [] } else { $rows | first | columns })
  layout $"data: ($name).csv" [
    (NAV {class: "muted"} (A {href: "/data"} "← all data tables"))
    (H1 $"($name).csv")
    (P {class: "muted"} $"($rows | length) rows, ($cols | length) columns")
    (if ($rows | is-empty) {
      (P {class: "muted"} "(empty)")
    } else {
      (TABLE {class: "data-table"}
        (THEAD (TR ...($cols | each {|c| TH $c })))
        (TBODY ...($rows | each {|r|
          (TR ...($cols | each {|c|
            let v = ($r | get -i $c | default "")
            if ($v | is-empty) { TD (SPAN {class: "muted"} "—") } else { TD (CODE $v) }
          }))
        })))
    })
  ]
}

# Index page listing every CSV in seed/ with row counts. Lets visitors
# browse the whole 37-file dataset that drives the demo.
export def page-data-index [
  entries: list        # [{name, rows}]
] {
  layout "Data" [
    (H1 "All data tables")
    (P {class: "muted"} $"($entries | length) CSV files in seed/. The 5 policy CSVs — actions, object_types, relations, permissions, event_types — drive Cedar codegen. The other 32 wire up the entity loader plus provide taxonomy for UI labels. Browse-only; editor for permissions.csv is at /policies/permissions.")
    (TABLE {class: "data-table"}
      (THEAD (TR (TH "Table") (TH "Rows") (TH "")))
      (TBODY ...($entries | each {|e|
        (TR
          (TD (CODE $e.name))
          (TD ($e.rows | into string))
          (TD (A {href: $"/data/($e.name)"} "browse")))
      })))
  ]
}

# Cedar `cedar check` playground. Pick (principal, action, resource_type,
# resource_id) and see the live decision. Exercises the full entity loader
# (not just Platform-scoped editor self-protection), so OWNER on Event,
# HEAD_COACH on Team, GUARDIAN on Player, etc. all get tried for real.
export def page-check [
  users: list                  # rows from users.csv
  actions: list                # rows from actions.csv
  object_types: list           # rows from object_types.csv
  form: record                 # current form values (null fields = unselected)
  result: any                  # null on initial GET; cedar check result record on POST
] {
  let user_options = ([{id: "", label: "(anonymous)"}] | append (
    $users | each {|u| {id: $u.id, label: $"($u.name_en) [($u.role_code)]"} }
  ))
  let result_block = (if $result == null { (DIV) } else {
    let decision = $result.decision
    let badge_cls = (if $decision == "allow" { "ok" } else { "deny" })
    (DIV {class: "check-result"}
      (H3
        (SPAN {class: $"badge ($badge_cls)"} ($decision | str upcase))
        $" "
        (SPAN $"($form.principal? | default "")  →  ($form.action? | default "")  on  ($form.resource_type? | default "")::\"($form.resource_id? | default "")\""))
      (if ($result.reasons | is-empty) { (DIV) } else {
        (DIV
          (P {class: "muted"} "Matching policies:")
          (UL ...($result.reasons | each {|r| (LI (CODE $r)) })))
      })
      (if ($result.errors | is-empty) { (DIV) } else {
        (DIV
          (P {class: "muted"} "Cedar errors:")
          (UL ...($result.errors | each {|e| (LI (CODE $e)) })))
      }))
  })

  layout "Check" [
    (H1 "Cedar check playground")
    (P {class: "muted"} "Pick a principal, action, and resource. The server runs `cedar check` against the live policies + materialised entity slice from the seed CSVs and shows the decision + matched policy ids. No auth gate -- this is a read-only diagnostic.")
    (FORM {method: "POST" action: "/check" class: "check-form"}
      (LABEL "Principal"
        (SELECT {name: "principal"}
          ...($user_options | each {|u|
            let attrs = (if ($u.id == ($form.principal? | default "")) { {value: $u.id selected: "selected"} } else { {value: $u.id} })
            (OPTION $attrs $u.label)
          })))
      (LABEL "Action"
        (SELECT {name: "action"}
          ...($actions | each {|a|
            let attrs = (if ($a.code == ($form.action? | default "")) { {value: $a.code selected: "selected"} } else { {value: $a.code} })
            (OPTION $attrs $"($a.code) -- ($a.name_en)")
          })))
      (LABEL "Resource type"
        (SELECT {name: "resource_type"}
          ...($object_types | each {|o|
            let cedar_name = ($o.code | str downcase | split row "_" | each {|w|
              (($w | str substring 0..0 | str upcase)) + ($w | str substring 1..)
            } | str join "")
            let attrs = (if ($cedar_name == ($form.resource_type? | default "")) { {value: $cedar_name selected: "selected"} } else { {value: $cedar_name} })
            (OPTION $attrs $cedar_name)
          })))
      (LABEL "Resource id"
        (INPUT {type: "text" name: "resource_id"
                value: ($form.resource_id? | default "global")
                placeholder: "global, evt_001, team_001, ply_001, ..."}))
      (BUTTON {type: "submit" class: "btn"} "Check"))
    $result_block
  ]
}

export def page-not-found [req: record] {
  layout "Not found" [
    (H1 "404 — Not found")
    (P $"No route for ($req.method) ($req.path)")
    (A {href: "/"} "Back home")
  ]
}
