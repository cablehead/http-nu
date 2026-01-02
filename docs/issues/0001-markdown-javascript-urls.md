# Markdown javascript: URL Injection

`.md` does not sanitize dangerous URL schemes in markdown links.

## Example

```markdown
[click me](javascript:alert(1))
```

Produces:

```html
<a href="javascript:alert(1)">click me</a>
```

Clicking executes the JavaScript.

## Why Current Design Doesn't Cover This

`.md` escapes raw HTML by intercepting `Event::Html` and `Event::InlineHtml`. Markdown links emit structured events (`Event::Start(Tag::Link {...})`), not HTML events—the URL passes through as data.

## Potential Fixes

1. **Intercept link events** - check `dest_url` for dangerous schemes (`javascript:`, `data:`, `vbscript:`)
2. **Allowlist schemes** - only permit `http:`, `https:`, `mailto:`, relative paths
3. **Rely on CSP** - Content-Security-Policy headers block inline script execution

## Tradeoffs

- Blocklists are fragile (many schemes, encoding tricks)
- Allowlists break legitimate use cases (custom protocols, bookmarklets)
- CSP is the modern defense but requires deployment configuration
