## Nushell Style

- `.last` returns null when the topic has no frames. Don't wrap it in
  `try { .last ... } catch { null }` -- the catch is dead weight. Same
  applies to other commands documented as returning null on miss; check
  before adding defensive `try`.
- Use `get -i` (or `get foo?`) for optional record fields rather than
  `try { $r.foo } catch { null }`.
- `-T` on `.cat` is an exact topic match, not a prefix. For prefix
  filtering use `.cat | where topic =~ '^...'`.

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

## View-transitions + `<button>` in WebKit

WebKit (Safari, playwright-webkit) animates the **opacity** of every
`<button>` element during a view-transition, regardless of CSS. The
button visibly pulses on every VT tick. Chromium doesn't. `<a>` and
`<div role="button">` are stable in both.

We hit this with the splash audio toggle: morse-code flash on each
1.2s SSE cadence, only in Safari. Spent a long time chasing VT capture
scope (`html { view-transition-name: none }`, parent stacking context,
`data-sse` proximity) before isolating it with bare-element probes
(`examples/2048/throwaway/_probe-types.mjs`): a bare `<button>` next
to the toggle oscillated 0.47 -> 1, `<a>` next to it stayed at 1.

**Rule.** For any clickable that lives near a view-transitioning region
and must look stable, render it as `<a>` (kbd-btn does this when
`--href` is set). For toggles that don't navigate, pass `--href "#"` and
`preventDefault()` in the click handler. The splash `.audio-toggle` is
the canonical instance.

Doesn't seem to be documented elsewhere; closest public report is the
Tailwind issue about `transition-opacity` on Safari buttons with icons,
which is hover-state, not VT-state.
