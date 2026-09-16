# Code syntax highlighting

How fenced code in the docs gets colored, and how to keep the
TextMate-scope -> stellar-token mapping in `base.css` complete as languages
change.

## Pipeline

`readme.nu` renders each page with `... | .md | get __html`. http-nu's `.md`
highlights fenced code via TextMate grammars (the `.highlight` engine),
emitting nested `<span class="<scope> <lang>">`, where `<scope>` is a TextMate
scope name flattened into space-separated classes - e.g.
`class="keyword operator comparison nu"` or `class="support function builtin bash"`.

## The mapping (base.css)

The `pre .<scope> { color: var(--code-<token>) }` rules map those scope classes
to stellar's `--code-*` syntax tokens. CSS multi-class selectors
(`.keyword.operator`, `.entity.name.struct`) mean the most-specific matching
rule wins; anything unmatched inherits `pre .source` -> `--code-fg`.

Stellar generates the `--code-*` palette (plus a coordinated dark variant) from
`colors.code` in `stellar.config.json`; we only consume the tokens.

## Documented languages

The README uses three fenced languages: **bash**, **nushell**, **rust**. The
mapping is audited against all three.

## Re-running the audit

1. Write an *exhaustive* sample per language covering every construct: comments
   (line / block / doc), strings (single / double / interpolated / raw / byte /
   char + escapes), numbers (int / float / hex / bin / oct / suffix), operators
   (arithmetic, comparison, logical, assignment), keywords (control +
   declaration), function calls, builtins / macros, variables / members /
   params, types, structural punctuation, and a deliberately invalid token.

2. Highlight each and capture the emitted scopes:

   ```bash
   http-nu eval -c '(open --raw sample.nu) | .highlight nu | get __html'
   ```

3. Cross-reference the emitted scopes against the CSS - resolve each scope to a
   token (most-specific wins), and report scopes that fall through to
   `--code-fg` plus `--code-*` tokens nothing uses:

   ```python
   import re
   base = open("base.css").read()
   mapped = []
   for m in re.finditer(r'((?:pre \.[\w.-]+,\s*)*pre \.[\w.-]+)\s*\{\s*color:\s*var\((--code-[\w-]+)\)', base):
       for sel in re.findall(r'pre (\.[\w.-]+)', m.group(1)):
           mapped.append((frozenset(sel.strip(".").split(".")), m.group(2)))

   def resolve(classes):                      # classes = a span's class list, minus the lang tag
       cs = set(classes)
       hits = [(len(req), tok) for req, tok in mapped if req <= cs]
       return max(hits)[1] if hits else "--code-fg (fall-through)"
   ```

## Findings

Every *meaningful* scope the three grammars emit maps to an appropriate token.
The only fall-throughs to `--code-fg` are **structural wrappers** -
`meta.braces / block / group / number`, `punctuation.section / separator` -
which is correct: their inner tokens are already colored, so the wrapper should
stay the default foreground (coloring it would just tint whitespace/brackets).

Assignments added during the audit (the gaps rust/nu exposed):

| scope | token |
| --- | --- |
| `entity.name.struct` / `trait` / `impl` | `--code-name-class` |
| `entity.name.label` (loop labels) | `--code-name-label` |
| `support.macro` (rust macros) | `--code-name-function` |
| `constant.other` | `--code-name-constant` |
| `constant.other.placeholder` (`{}` / `{name}`) | `--code-string-escape` |
| `invalid` (bad identifiers) | `--code-error` |

### Stellar code tokens we don't use

These have no scope in bash / nushell / rust (they serve languages and
constructs we don't document), so they are intentionally unused. They would
light up if we added the relevant language:

- markup / diff / markdown: `--code-emph`, `--code-strong`, `--code-deleted`,
  `--code-inserted`, `--code-comment-special`
- HTML / XML: `--code-name-tag`, `--code-name-attribute`, `--code-name-entity`
- other languages: `--code-name-decorator` (Python), `--code-name-exception`,
  `--code-name-namespace`, `--code-keyword-const` (nu booleans scope as builtins)
- specialty strings: `--code-string-affix`, `--code-string-doc`,
  `--code-string-regex`

## Adding a language

Add a fenced sample, re-run the audit, and map any new non-structural
fall-throughs to the closest `--code-*` token. If a needed concept has no
matching stellar token, that's a `colors.code` config question, not a CSS one.
