# nu_plugin_cedar

Nushell plugin exposing Cedar policy authorization. Wraps the
[cedar-policy](https://crates.io/crates/cedar-policy) Rust crate so any
Nushell script can call `cedar authorize`.

Intended consumer in this repo: `examples/cedar-admin/` via the
`http-nu/cedar` stdlib module (`src/stdlib/cedar/mod.nu`).

## Install

From the http-nu workspace root:

```sh
cargo build -p nu_plugin_cedar
```

Output binary: `target/debug/nu_plugin_cedar`.

In a Nushell script (or interactively):

```nu
plugin add target/debug/nu_plugin_cedar
plugin use cedar
```

## One command

```
cedar authorize <record>
  -> {decision: "allow"|"deny", reasons: [String], errors: [String]}
```

Input record fields:

| field      | type   | required | meaning                                                         |
|------------|--------|----------|-----------------------------------------------------------------|
| principal  | string | yes      | Cedar EntityUid, e.g. `'User::"alice"'`                         |
| action     | string | yes      | Cedar EntityUid, e.g. `'Action::"EDIT_EVENT"'`                  |
| resource   | string | yes      | Cedar EntityUid, e.g. `'Event::"evt_001"'`                      |
| policies   | string | yes      | Cedar policy text (one or many `permit` / `forbid` blocks)      |
| entities   | string | no       | Cedar entities JSON; defaults to `"[]"`                          |
| context    | string | no       | Cedar context JSON; defaults to `"{}"`                           |

Entities and context are passed as JSON strings; the
`http-nu/cedar` middleware module handles the nu-record-to-JSON
conversion so user code doesn't see it.

## Caching

The parsed `PolicySet` is cached per policy-text hash. The 186-permission
remy-sport policy set parses once at first call and stays warm for the
life of the plugin process (which nushell keeps alive across calls).

## Why string fields, not nu records

Keeps the plugin's wire format dead simple (no nu-Value -> serde-Value
recursive conversion in Rust). The middleware does `to json -r` before
calling, which is one line of nushell and free at the call site. We can
add a typed-record path later without breaking the string path.
