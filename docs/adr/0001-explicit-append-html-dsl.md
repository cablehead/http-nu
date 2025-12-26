> **Note:** See [ADR-0003](0003-html-dsl-design.md) for the consolidated HTML DSL design.

# Explicit Append in HTML DSL

## Context

The HTML DSL originally piped tags together implicitly:

```nushell
_ul { _li "one" | _li "two" }
```

Tags implicitly appended to `$in`, which broke in unexpected ways:

```nushell
_ul { _li "hi" | ("foo" | _li $in) }
# Got: <ul>foo<li>foo</li></ul>
```

`_li $in` appended `<li>foo</li>` to its `$in` (also "foo"), producing `foo<li>foo</li>`.

## Decision

Use explicit `append` for siblings:

```nushell
_ul { _li "one" | append (_li "two") }
_ul { 1..3 | each {|n| _li $"Item ($n)" } }  # each returns list naturally
```

## Rationale

- `each` returns a list - works without special handling
- Explicit structure mirrors HTML's tree nature
- No magic - what you write is what happens
- Verbose but predictable beats concise but surprising
