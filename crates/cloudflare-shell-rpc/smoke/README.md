# smoke

End-to-end smoke test for `cloudflare-shell-rpc`. Lives alongside
the demos because it tests the whole subsystem (server + demo +
service binding + DO + R2 round-trip), not any single piece.

```bash
# bring everything up first
mise run cf:fs:up

# smoke against JS demo (:8789) and Rust demo (:8790)
mise run cf:fs:smoke
mise run cf:fs:smoke:rust

# both, with up/down handled
mise run cf:fs:smoke:all

# tear down
mise run cf:fs:down
```

## Direct invocation

```bash
nu crates/cloudflare-shell-rpc/smoke/run.nu \
  --url http://127.0.0.1:8789

nu crates/cloudflare-shell-rpc/smoke/run.nu \
  --url https://cloudflare-shell-rpc-demo-js.example.workers.dev
```

## What it covers

Sequenced steps -- each must pass before the next:

| Step | Assertion |
|------|-----------|
| `PUT /fs/<ns><path>`              | accepted     |
| `GET /fs/<ns><path>`              | body matches what was PUT |
| `GET /stat/<ns><path>`            | `kind == "file"` |
| `POST /mkdir/<ns>/sub`            | accepted     |
| `GET /list/<ns>/`                 | entries include the file and the sub-dir |
| `DELETE /fs/<ns><path>`           | accepted     |
| `GET /fs/<ns><path>` (post-rm)    | errors out (4xx/5xx) |
| `GET /list/bad-ns/`               | errors out -- proves `VALID_NAMESPACE` is enforced server-side, closing the SQL-identifier-injection vector |

If any step fails, the script `error make`s with a description --
mise surfaces it as a task failure.
