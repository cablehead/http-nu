# Markdown and Highlight Escaping

Extends [ADR-0002](0002-html-escaping-strategy.md)'s `{__html: ...}` convention
to `.md` and `.highlight` commands.

## Background

ADR-0002 established:

- Strings are escaped by default in the HTML DSL
- `{__html: ...}` marks trusted content that bypasses escaping
- Secure by default, opt-in for raw HTML

## Problem

`.md` and `.highlight` convert text to HTML. Without escaping:

```nushell
$req.body | .md  # user submits: <script>evil()</script>
# Output: {__html: "<script>evil()</script>"}  ← XSS vulnerability
```

## Decision

### `.highlight`

Always escapes input. Code is never trusted as HTML—syntect's
`ClassedHTMLGenerator` HTML-escapes content automatically. Returns
`{__html: ...}` for DSL composition.

### `.md`

Accepts two input types:

| Input             | Trust     | HTML Handling   |
| ----------------- | --------- | --------------- |
| `"string"`        | Untrusted | Escape raw HTML |
| `{__html: "..."}` | Trusted   | Pass through    |

Implementation intercepts pulldown-cmark events:

```rust
Event::Html(html) => {
    if trusted { Event::Html(html) }
    else { Event::Text(html) }  // push_html escapes Text
}
```

Markdown syntax using `<>` (autolinks, etc.) still works—pulldown-cmark emits
these as structured events, not `Event::Html`.

## Examples

```nushell
# Untrusted - escaped
"<script>evil()</script>" | .md | get __html
# → &lt;script&gt;evil()&lt;/script&gt;

# Trusted - passed through
{__html: "<strong>bold</strong>"} | .md | get __html
# → <p><strong>bold</strong></p>

# Autolinks still work
"<http://example.com>" | .md | get __html
# → <p><a href="http://example.com">...</a></p>
```

## Tradeoffs

- Records without `__html` key error explicitly
- Consistent with HTML DSL's `{__html: ...}` convention
- No `--unsafe` flag needed—the convention already exists
