## Nushell Style

- `.last` returns null when the topic has no frames. Don't wrap it in
  `try { .last ... } catch { null }` -- the catch is dead weight. Same
  applies to other commands documented as returning null on miss; check
  before adding defensive `try`.
- Use `get -i` (or `get foo?`) for optional record fields rather than
  `try { $r.foo } catch { null }`.
- `-T` on `.cat` is an exact topic match, not a prefix. For prefix
  filtering use `.cat | where topic =~ '^...'`.

## Markup + CSS: the hammer test

Before adding anything, ask "would it hurt more to skip this than to
add it?" If skipping wins, skip. These four moves are the worst
offenders -- bias hard against them:

1. **Hand-rolling a markup pattern.** If we already have a server-side
   component for this shape (`kbd-btn`, `breadcrumb`, `render-board`,
   `kbd-btn`, `render-card-from-state`, ...), use it. Hand-rolling
   means a second source of truth that drifts. List in
   `tfe/render.nu`; check there first. /design/<slug> previews the
   live components.
2. **Adding a new component** when an existing one almost fits. Extend
   the existing one or accept it doesn't perfectly fit. Two
   90%-overlapping components cost more than one slightly-imperfect
   one.
3. **Adding more-specific CSS to an element.** Why isn't the cascade
   doing it? If the parent already sets the value (font-family,
   font-size, color), don't restate on the child. If you find yourself
   writing `body.X .Y` or `.parent .child` -- ask if it can be plain
   `.Y` / `.child`. Specificity bloat makes future overrides harder.
4. **Adding a class.** Why doesn't the semantic element carry the
   meaning? `<header>`, `<nav>`, `<code>`, `<kbd>`, `<samp>`,
   `<output>` already imply role + default styling. Classes are for
   things HTML doesn't have a word for.

When in doubt: **lean into the markup, let it decide.** The CSS file
should mostly be "set defaults on elements + small set of components."
Leaf rules and `body.X` prefixes are usually a smell.
