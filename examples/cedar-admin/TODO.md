# TODO

Deferred work for cedar-admin. KISS list; one paragraph per item with the
reasoning so future-us doesn't have to re-derive the "why".

## D -- CI tooling: `cedar:regen` task + `cedar validate` plugin command

Three pieces:

- **`mise run cedar:regen`** -- writes generated `policies.cedar` +
  `policies.cedarschema` to `examples/cedar-admin/build/` (gitignored).
  Lets you diff drift, paste into Cedar Playground, run cedar-policy-cli
  against the same files.
- **`cedar validate` plugin command** -- wraps `cedar_policy::Validator`
  for strict-mode policy/schema validation. Catches "policy references
  undeclared action" at CI time instead of as an opaque
  `Authorizer.is_authorized` failure at request time.
- **`test/test.nu` assertion** -- `cedar validate` against the generated
  output every test run; fails loud on codegen drift.

Why deferred: current test suite catches behavioural regressions (admin
allow, coach deny). D closes the structural-drift gap. Worth doing if the
demo ships to anyone else or the CSVs grow.

Effort: ~2 hours (15 lines nu + ~40 lines Rust plugin + 3 line test).

## I -- cedar-for-agents integration (long-tail)

[cedar-policy/cedar-for-agents](https://github.com/cedar-policy/cedar-for-agents)
exposes Cedar's analysis capabilities through an MCP server. Future
integration would let an AI agent ask cedar-admin's policy state
questions like "who can edit evt_001?" or "what would change if I remove
the PLATFORM_ADMIN row?". Out of scope for v0; revisit when MCP is a
first-class deliverable.

## Production-grade extensions (out of scope for the demo)

- Real auth (Better Auth, OAuth, magic links) -- drop the user-picker
- Multi-tenant policy isolation (per-org seed)
- Deploy target + secrets via fnox + observability (log every Cedar
  decision with the matched policy id)
- Backup/restore for xs frames

## Refactor candidates (small, do as you touch the code)

- `serve.nu` GET handlers for policy CSVs do two cedar-gate calls
  (VIEW_OWN_PERMISSIONS + EDIT_POLICY); only the second is used.
  Drop the VIEW gate or use it to short-circuit if denied.
- Per-request codegen runs ~30-50ms; cache by mtime of the source CSVs.
- Components extraction -- `lib/components.nu` with reusable
  `data-table`, `role-badge`, `breadcrumb` so `lib/views.nu` reads as
  composition rather than long inline DSL trees.

---

## Done

The following were in this file earlier; landed in track A (demo-complete):

- ~~F -- Real `/matrix` view~~ (action-listing per object_type, with subtype letters)
- ~~D' -- `/check` playground route~~ (form + live cedar check + decision/reasons/errors)
- ~~G -- Surface remaining CSVs~~ (`/data` index + `/data/<name>` generic table browser)
- ~~E -- Editors for the other 4 policy CSVs~~ (generic /policies/<name> for permissions, actions, relations, object_types, event_types)
- ~~H -- REV cache-buster on static assets~~ (per-server-restart token via `$env.STATIC_REV`)
