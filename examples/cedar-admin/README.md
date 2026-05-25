# cedar-admin

A live editor for Cedar authorization policies, gated by the policies
it edits. Self-referential demo of [Cedar](https://github.com/cedar-policy/cedar)
on http-nu + cross.stream + Datastar.

Source CSVs are a real-world snapshot from
[remy-sport](https://github.com/joeblew999/remy-sport-biz/tree/main/data/seed)
(a basketball platform): 37 files, 190 permission rows, 103 distinct policies.

## Run

```sh
mise run ex:cedar-admin
```

Then visit <http://127.0.0.1:3001>, sign in as `usr_admin_001`, open
`/policies/permissions` in two tabs, edit in one, watch the other update
without reload.

## Try the demo

1. `/` -- landing, anonymous.
2. `/login` -- pick any of the 11 active users.
3. `/me` -- shows current session.
4. `/matrix` -- per-object-type action listing with subtype scoping (T/L/K/Sh),
   plus the raw generated `policies.cedar` + `.cedarschema` in collapsibles.
5. `/policies/<name>` for `permissions`, `actions`, `relations`,
   `object_types`, `event_types` -- generic editor for the 5 CSVs that
   drive Cedar codegen. Edit form + delete buttons appear only when signed
   in as an admin. Edits propagate live to other open browsers via SSE.
6. `/data` -- index of all 35 seed CSVs; `/data/<name>` browses any of them
   as a generic table (read-only).
7. `/check` -- Cedar check playground. Pick a principal + action + resource
   and see the live decision + matched policy ids + errors. Exercises the
   full entity loader (OWNER, HEAD_COACH, GUARDIAN, ...).

Non-admin sign-ins (coach, player, etc.) get a 403 on any policy edit/delete --
the same Cedar rules that gate the editor UI also gate the underlying routes.

## Round-trip

```
edit form submit  ->  POST /policies/permissions  (cedar EDIT_POLICY check)
                            |
                            v
                      write seed/permissions.csv
                            +
                      append xs frame `cedar.policy.edited`
                            |
                            v
              SSE projection: re-render tbody
                            |
                            v
              datastar-patch-elements over open SSE  ->  all admin browsers
```

The next GET reads the updated CSV (per-request codegen), so subsequent
authz checks see the new rule too.

## Self-protection footgun

The demo's editor is gated by `EDIT_POLICY` / `DELETE_POLICY`, both granted
to `PLATFORM_ADMIN`. If you delete the row that grants `EDIT_POLICY` to
`PLATFORM_ADMIN`, you lock yourself out -- no admin can edit anymore.
Recovery: edit `seed/permissions.csv` directly and restart.

## File layout

| Path | What |
|---|---|
| `serve.nu` | Routes; bootstrap; cedar-gate + audit helpers |
| `auth.nu` | Session cookie <-> xs `session.<token>` frame <-> `users.csv` lookup |
| `lib/codegen.nu` | CSVs -> `policies.cedar` + `policies.cedarschema` (99 + 4 policies) |
| `lib/entity-loader.nu` | Materialises Cedar entities from domain CSVs per `relations.csv` |
| `lib/views.nu` | http-nu/html DSL renderers; `permissions-tbody` is reused for SSE patches |
| `static/styles.css` | Mobile-first, dark-aware, ~230 lines, no framework |
| `seed/*.csv` | 37 CSVs copied from remy-sport-biz; the source of truth |
| `test/check.sh` | Unit + Playwright e2e umbrella |
| `test/test.nu` | Pure-logic unit tests (codegen, auth, entity-loader) |
| `test/test.mjs` | HTTP + Playwright browser e2e (incl. SSE live-update) |

## Why all 37 CSVs?

5 drive Cedar codegen (`actions`, `object_types`, `relations`, `permissions`,
`event_types`). 3 drive identity (`users`, `roles`, `user_statuses`). The
other 29 wire up the entity loader (events, teams, players, etc. -- everything
named in `relations.csv`'s `derived_from` column) and provide taxonomy for
the UI. v0 only surfaces `permissions` as editable; the rest are committed
so the demo is a real working slice of remy-sport, not a contrived sample.

## Two added actions you should know about

`seed/actions.csv` + `seed/permissions.csv` carry **4 actions added for the
editor itself** (additive only; the original 65 + 186 rows from remy-sport
are untouched):

- `VIEW_ACCESS_MATRIX` -- granted to `PUBLIC`
- `VIEW_OWN_PERMISSIONS` -- granted to `ANY_SIGNED_IN`
- `EDIT_POLICY` -- granted to `PLATFORM_ADMIN`
- `DELETE_POLICY` -- granted to `PLATFORM_ADMIN`

## Tests

```sh
mise run cedar-admin:test
```

Runs `test.nu` (codegen + auth + entity-loader assertions) and `test.mjs`
(Playwright e2e: login flow, edit-gated-by-cedar, SSE live update). 22
checks total. Chromium-dependent checks skip cleanly if chromium isn't
installed.

## Future

See [TODO.md](./TODO.md) for the deferred backlog (CI tooling, refactor
candidates, production-grade extensions, cedar-for-agents MCP integration).
