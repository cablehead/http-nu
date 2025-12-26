# HTML DSL Design

## TL;DR

```nushell
# Uppercase tags, lisp-style nesting
HTML (
  HEAD (TITLE "Demo")
  BODY (
    H1 "Hello"
    UL { 1..3 | each {|n| LI $"Item ($n)" } }
  )
)

# Auto-escapes untrusted input via {__html} wrapper
DIV $user_input  # escaped: &lt;script&gt;...

# Jinja2 DSL for compiled templates
let tpl = UL (_for [item items] (LI (_var "item")))
let compiled = $tpl.__html | .mj compile --inline $in
{items: [a b c]} | .mj render $compiled  # fast
```

---

## Context

http-nu embeds an HTML DSL in nushell. This ADR documents the design decisions around tag naming, XSS prevention, and the path to performance.

## Tag Naming

We tried several naming conventions before settling on uppercase-only (`DIV`).

### Approach 1: Underscore prefix (`_div`)

```nushell
_html (
  _head (_title "Demo")
  _body (
    _h1 "Hello"
    _ul { 1..3 | each {|n| _li $"Item ($n)" } }
  )
)
```

Lisp-style with variadic args. Works well for tree structures.

### Approach 2: Pipe + append (`_div | +div`)

```nushell
_div {class: "card"} {
  _div {class: "title"} "Sunset"
  | +div {class: "author"} "Photo by Alice"
  | +div {class: "date"} "2025-12-15"
}
```

The `+tag` variants append to pipeline. Designed for sibling elements.

**Problems:**
- Two ways to do the same thing (`_div` vs `+div`)
- Mental overhead deciding which to use
- Doubles the API surface (100+ tag functions become 200+)

### Approach 3: Uppercase only (`DIV`)

```nushell
HTML (
  HEAD (TITLE "Demo")
  BODY (
    H1 "Hello"
    UL { 1..3 | each {|n| LI $"Item ($n)" } }
  )
)
```

**Decision:** Use uppercase only.

**Rationale:**
- One way to do things
- Visually distinct from nushell builtins (`div` vs `DIV`)
- Lisp-style nesting reads naturally
- `VAR` for HTML `<var>` tag leaves `_var` available for Jinja2

## Escaping Strategy

### The Problem

User input can contain malicious HTML:

```nushell
let evil = "<script>alert(document.cookie)</script>"
DIV $evil  # XSS if not escaped
```

But nested tags must pass through unescaped:

```nushell
DIV (SPAN "safe")  # must NOT escape the <span>
```

Both arrive as plain string arguments. We need to distinguish trusted from untrusted.

### Solution: Record wrapper

Tags return `{__html: "..."}`. Plain strings get auto-escaped in `to-children`:

```nushell
> DIV "hello"
{__html: "<div>hello</div>"}

> DIV "<script>bad</script>"
{__html: "<div>&lt;script&gt;bad&lt;/script&gt;</div>"}

> DIV (SPAN "nested")
{__html: "<div><span>nested</span></div>"}
```

The `__html` field marks trusted content. Strings without the wrapper are escaped.

## Performance

Benchmarks rendering a 100-row user table:

| Approach | Time | vs baseline | Notes |
|----------|------|-------------|-------|
| string-no-escape | 186ms | — | no XSS protection |
| record-escaped | 202ms | +8.6% | `{__html}` wrapper |
| custom-type | 1275ms | +585% | Rust boundary overhead |
| mj-compiled | 2.5ms | -98.7% | compile + render |
| mj-render-only | 0.86ms | -99.5% | pre-compiled |

**Observations:**
1. Record-wrapped escaping adds ~9% overhead — acceptable for XSS protection
2. Rust custom types are 6x slower due to boundary crossing per tag
3. Compiled Jinja2 templates are 74-216x faster than nushell DSL

### Why the DSL is slow

Each tag call evaluates nushell code. A 100-row table with 4 columns means ~400 tag invocations, each doing:
- Argument parsing
- Type checking (`describe -d`)
- String concatenation
- Record construction

### Why Jinja2 is fast

1. Template compiled once to bytecode
2. Rendering is pure Rust, no nushell calls
3. Single command invocation per render
4. Data passed as record, not interpolated string-by-string

## Jinja2 Integration

Rather than optimize the nushell interpreter, we leverage minijinja. The DSL gains Jinja2 control flow:

```nushell
# _var emits {{ expr }}
_var "user.name"  # {{ user.name }}

# _for emits {% for %}
UL (_for [item items] (LI (_var "item")))
# <ul>{% for item in items %}<li>{{ item }}</li>{% endfor %}</ul>

# _if emits {% if %}
_if "user.admin" (DIV "Admin Panel")
# {% if user.admin %}<div>Admin Panel</div>{% endif %}
```

Full workflow:

```nushell
# Author template with nushell ergonomics
let tpl = UL (_for [item items] (LI (_var "item")))

# Compile once
let compiled = $tpl.__html | .mj compile --inline $in

# Render fast (many times)
{items: [a b c]} | .mj render $compiled
# <ul><li>a</li><li>b</li><li>c</li></ul>
```

**Trade-off:** Jinja2 templates can't use nushell closures or `each`. The `_for`/`_if`/`_var` elements generate static template strings. For dynamic nushell logic, use the runtime DSL and accept the ~200ms overhead.

## Summary

1. **Tags:** Uppercase only (`DIV`, `SPAN`, `UL`)
2. **Escaping:** `{__html}` wrapper, ~9% overhead, secure by default
3. **Performance:** Use `.mj compile` + `.mj render` for hot paths
4. **Jinja2 DSL:** `_for`, `_if`, `_var` for compiled templates
