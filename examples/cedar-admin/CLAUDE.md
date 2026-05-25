## Before declaring a cedar-admin change done

Run `mise run cedar-admin:test`. The umbrella script runs:
- `test.nu` -- pure unit tests (codegen, auth, entity-loader, schema)
- `test.mjs` -- HTTP probes + Playwright browser e2e incl. SSE live-update

22 checks total. Browser-dependent checks skip cleanly if chromium isn't
installed locally.

## Don't pass `-w` (watch mode)

The serve loop writes `seed/permissions.csv` on every edit, which trips
http-nu's source watcher and kills the in-flight SSE connection
mid-stream -- the patch for the edit that JUST happened gets lost. The
`mise run ex:cedar-admin` task launches without `-w` for this reason.
For code reload during development, restart manually.

## SSE `--new` race

`.cat -T <topic> --follow --new` only captures frames that land AFTER
subscription. The test must wait for the INITIAL datastar-fetch event
(proof that the subscription is live) before triggering the edit that
should produce the second event. See `test.mjs`'s `waitForFunction` on
`window.__dsEvents` -- without it the live-update assertion races and
flakes.

## Datastar 1.0.1, not RC.5

http-nu ships Datastar v1.0.1. The TodoMVC reference in `xs/examples/`
is RC.5 and has breaking-changed syntax. For 1.0.1:
- `data-init="@get('/path')"` -- runs once on mount (RC.5: `data-on-load`)
- `data-on:click` -- colon, not dash
- `@get`, `@post` accept retry options as the 2nd arg

## Multi-line http-nu/html DSL needs outer parens

`TR (TD ...) (TD ...)` across newlines parses as separate statements --
only the last `(TD ...)` becomes the closure return. Wrap with outer
parens: `(TR (TD ...) (TD ...))`. Same trap with `HTML (HEAD ...) (BODY ...)`.

## Source-of-truth CSVs

`seed/` has 37 CSVs copied from `joeblew999/remy-sport-biz/data/seed`,
plus 4 actions and 4 permissions added for the editor's own gating
(`VIEW_ACCESS_MATRIX`, `VIEW_OWN_PERMISSIONS`, `EDIT_POLICY`,
`DELETE_POLICY`). Don't delete these four -- removing them locks
admins out of the editor. If a test damages permissions.csv:

```nu
let valid = (open examples/cedar-admin/seed/actions.csv | get code)
open examples/cedar-admin/seed/permissions.csv
| where action_code in $valid
| collect
| save -f examples/cedar-admin/seed/permissions.csv
```

That drops any orphan rows (e.g. `TEST_ADD_BY_E2E`) without touching
real seed data.

## Test cleanup: never `delete-last-row`

The test add+delete cycle MUST look up the row index by `action_code`,
not by tbody length minus one. A previous-run orphan would shift the
index and the cleanup would silently delete a real seed row (we lost
`DELETE_POLICY` to this once). See `test.mjs`'s `findIndex` lookup.
