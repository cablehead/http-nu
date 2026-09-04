# Markdown with Mermaid

This page is a single markdown file rendered by http-nu's `.md` command.
Fenced code blocks tagged `mermaid` become live diagrams in the browser.
Every other fence is syntax highlighted on the server as usual.

## A flowchart

```mermaid
graph TD
    A[Request] --> B{Route?}
    B -->|GET /| C[Render page.md]
    B -->|GET *.js| D[Static assets]
    C --> E[Browser upgrades mermaid fences]
```

## A sequence diagram

```mermaid
sequenceDiagram
    participant Browser
    participant http-nu
    Browser->>http-nu: GET /
    http-nu-->>Browser: HTML from .md
    Browser->>Browser: find pre > code.language-mermaid
    Browser->>Browser: swap in <mermaid-diagram>
    Browser->>CDN: import mermaid.esm.min.mjs
    CDN-->>Browser: mermaid
    Browser->>Browser: render SVG
```

## A regular code block

This one stays a `<pre>` and is highlighted by the server:

```nushell
def render [path: string]: nothing -> record {
  open --raw $path | decode utf-8 | .md
}
```

Inline HTML like <script>alert(1)</script> is escaped because the markdown
is passed to `.md` as a plain string, not as `{__html: ...}`.
