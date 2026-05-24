## Before declaring a 2048 change done

Run `examples/2048/test/check.sh`. The top-level `scripts/check.sh`
only runs the pure unit tests (`test.nu`); the per-example
`check.sh` adds the SSE-pipeline tests (`test-sse.nu`, requires
`--store`) and the browser e2e (`test.mjs`, chromium). The latter
catches whole-page regressions that pure unit tests miss -- and
caught the live `/sse-wc` hang once we wired test-sse.nu in.

## Nushell Style

- `.last` returns null when the topic has no frames -- verified in
  `xs/src/nu/commands/last_command.rs` (returns `PipelineData::Empty`).
  Don't wrap it in `try { .last ... } catch { null }` -- the catch is
  dead weight. `.get` is different: it raises a `ShellError` on miss,
  so `try { .get $id } catch { null }` is correct there. When in doubt,
  read the command's source before adding defensive `try`.
- Use `get -i` (or `get foo?`) for optional record fields rather than
  `try { $r.foo } catch { null }`.
- For chained `.last "topic" | get meta.field` where both the lookup
  and the field may be missing, prefer the optional-access form:
  `.last "topic" | get meta?.field? | default <fallback>`. That's
  one expression, no try, no temporary binding, and surfaces the
  field name in plain sight. Avoid the older
  `try { .last "topic" | get meta.field } catch { <fallback> }`.
- `let foo = (expr)` -- parens are NOT needed for single-line let
  values, even when the value is a pipeline. `let s = resolve-session
  $req`, `let token = $req | cookie parse | get session?`, and `let
  next_tabs = $st.tabs | upsert $tab_id $entry` all work bare.
  Reach for parens only when:
    * The value spans multiple lines (`let body = ([...] | layout ...)`
      with the closing `)` on a later line).
    * You're forcing operator precedence next to a non-pipe operator
      (e.g. `let pos = (($f.meta | ... | into int) mod $n)` where
      without the inner group, `mod` would bind to the wrong side).
    * The value has a boolean/arith expression with sub-pipelines:
      `let ok = ($x | str starts-with "a") and ($y | str ends-with
      "b")` -- the inner parens isolate the pipes from `and`.
  Default to bare; add parens to grouping that actually grouping.
- `-T` on `.cat` is an exact topic match, not a prefix. For prefix
  filtering use `.cat | where topic =~ '^...'`.
- **Never bind a streaming pipeline to `let`.** In Nushell `let x =
  <pipeline>` **collects** the pipeline into a value before binding,
  so `let s = .cat --follow ...` hangs forever on an infinite stream.
  Pipe streams straight into their consumer (`... | interleave { ... }
  | to sse`), or use the canonical two-stream shape from the README:
  `null | interleave { stream1 } { stream2 } | to sse`. See
  `examples/2048/test/test-sse.nu` (T3) for the regression guard.
- `interleave` takes **closures** (`{|| stream }`), not stream
  *values*. Passing a stream value errors at runtime when the engine
  tries to invoke it as a closure ("can't convert ... to Closure"),
  which inside an SSE handler manifests as a silent hang from upstream
  before the error is ever raised. test-sse.nu T1 guards this.

## Markup + CSS: hammer test

For each, ask: does skipping hurt more than adding?

- **Hand-rolling markup.** Use the server-side component
  (`kbd-btn`, `breadcrumb`, `render-board`, `render-card-from-state`,
  ...). Check `tfe/render.nu`. Browse /design.
- **Adding a component.** Extend an existing one before adding a
  90%-overlap sibling.
- **More specific CSS.** Why isn't the cascade doing it? Drop
  `body.X .Y` to `.Y`. Don't restate what the parent set.
- **Adding a class.** Why doesn't `<header>`, `<nav>`, `<code>`,
  `<kbd>`, `<output>` carry it? Classes are for things HTML lacks.

Lean into the markup. Let the cascade decide.
