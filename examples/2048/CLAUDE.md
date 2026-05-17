## Nushell Style

- `.last` returns null when the topic has no frames. Don't wrap it in
  `try { .last ... } catch { null }` -- the catch is dead weight. Same
  applies to other commands documented as returning null on miss; check
  before adding defensive `try`.
- Use `get -i` (or `get foo?`) for optional record fields rather than
  `try { $r.foo } catch { null }`.
- `-T` on `.cat` is an exact topic match, not a prefix. For prefix
  filtering use `.cat | where topic =~ '^...'`.
