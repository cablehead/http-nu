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
