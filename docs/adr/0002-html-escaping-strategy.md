> **Note:** See [ADR-0003](0003-html-dsl-design.md) for the consolidated HTML DSL design.

# HTML Escaping Strategy

## Context

User input can contain malicious HTML:

```nushell
let $evil = "<script>alert(document.cookie)</script>"
_div $evil
# renders: <div><script>alert(document.cookie)</script></div>
# XSS attack executes in browser
```

The fix is escaping `<`, `>`, `&` in `to-children` at `src/stdlib/html/mod.nu`:

```nushell
'string' => ($in | escape-html)  # <script> becomes &lt;script&gt;
```

## The Dilemma

```nushell
_div $evil (_footer "my content")
```

Both arguments arrive as strings. We can't tell that arg1 must NOT be trusted, but arg2 MUST be trusted—escaping `(_footer ...)` would break the `<footer>` tag we just generated.

We need to distinguish safe from unsafe strings.

## Options

### A: `esc` for untrusted input

`_div (esc $evil)` escapes, `_div (_span "x")` works unchanged. Current API preserved. Opt-in safety—easy to forget.

### B: `| html` at end

Tags return records internally:

```nushell
> _div "hello"
{__html: "<div>hello</div>"}

> _div (_span "nested")
{__html: "<div><span>nested</span></div>"}
```

The `| html` command unwraps to string:

```nushell
> _div "hello" | html
<div>hello</div>
```

Strings without `__html` wrapper get escaped automatically. Secure by default. Requires piping final output.
