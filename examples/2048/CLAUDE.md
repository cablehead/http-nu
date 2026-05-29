## Before declaring a 2048 change done

Run `examples/2048/test/check.sh`. The top-level `scripts/check.sh`
only runs the pure unit tests (`test.nu`); the per-example
`check.sh` adds the SSE-pipeline tests (`test-sse.nu`, requires
`--store`) and the browser e2e (`test.mjs`, chromium). The latter
catches whole-page regressions that pure unit tests miss -- and
caught the live `/sse-wc` hang once we wired test-sse.nu in.

## Nushell Style

- `.last` on a topic with no frames returns an **empty pipeline**
  (`PipelineData::Empty`, see `xs/src/nu/commands/last_command.rs`), NOT a
  `null` value. The distinction is load-bearing:
    * **Bound** -- `let f = .last "topic"` -- the empty pipeline collapses
      to `null`, so `let f = .last "topic"; if $f == null { ... }` is
      correct and needs no `try`. The catch in `try { .last ... } catch
      { null }` is dead weight here.
    * **Piped into a command or cell path** -- `.last "topic" | get ...`
      or `(.last "topic").field` -- the empty pipeline is NOT null. `get`
      raises "Pipeline empty" and a cell path raises "empty pipeline
      doesn't support cell paths", and any trailing `| default` never
      runs. Guard it: `.last "topic" | default {} | get ...` (coerces the
      empty pipeline to `{}`), or bind first and check for null.
  `.get` is different: it raises a `ShellError` on miss, so
  `try { .get $id } catch { null }` is correct there. When in doubt, read
  the command's source before adding defensive `try`.
- Use `get -i` (or `get foo?`) for optional record fields rather than
  `try { $r.foo } catch { null }`.
- For chained `.last "topic" | get meta.field` where the topic may be
  empty AND the field may be missing, guard the empty pipeline first:
  `.last "topic" | default {} | get meta?.field? | default <fallback>`.
  The leading `| default {}` turns a missing topic's empty pipeline into
  `{}` so `get` can't crash; `meta?.field?` handles a missing field; the
  trailing `| default` supplies the fallback. WITHOUT the `| default {}`,
  an empty topic raises "Pipeline empty" at `get` and the trailing
  `default` never runs (this is exactly how the leaderboard-actor's
  `start:` expression silently died). If the topic is *guaranteed*
  non-empty you can drop the `| default {}`, but add it whenever
  existence isn't certain. Either way, prefer this over the older
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
