# Markdown with Mermaid fences

Render a markdown file with http-nu's `.md` command and turn its
```` ```mermaid ```` fences into live [Mermaid](https://mermaid.js.org/)
diagrams in the browser.

## Run

```bash
http-nu :3001 examples/markdown-mermaid/serve.nu
```

Open http://localhost:3001.

## How it works

`.md` has no per-language hook. It runs every fenced block through the
syntax highlighter, and since there is no mermaid grammar, a mermaid fence
comes out as plain text:

```html
<pre><code class="language-mermaid"><span class="text plain">graph TD
  A --&gt; B
</span></code></pre>
```

The server leaves that alone. The browser does the upgrade:

1. `GET /` reads `content/page.md`, pipes it through `.md`, and drops the
   result into the page body with the HTML DSL. The markdown is passed as a
   plain string, so any inline HTML in it is escaped.

2. `assets/mermaid-fences.js` runs on load. It finds every
   `pre > code.language-mermaid`, reads the diagram source via
   `textContent` (which strips the highlighter's spans and decodes
   entities), and replaces the `<pre>` with a `<mermaid-diagram>` element.

3. `<mermaid-diagram>` is the web component from the
   [mermaid-editor](../mermaid-editor/) example, served straight from that
   example's assets directory. It lazy-loads Mermaid.js from a CDN and
   renders the SVG into its shadow DOM.

Non-mermaid fences are untouched and keep their server-side highlighting.
The page includes the `GitHub` theme CSS via `.highlight theme GitHub`.

## Avoiding the flash of diagram source

Module scripts are deferred, so without help the browser paints the raw
mermaid source in a `<pre>`, then swaps it for the diagram. The page
avoids that in three steps:

- A classic inline script in `<head>` adds a `js` class to `<html>`. It
  runs synchronously during parsing, before the body is rendered.
- CSS scoped to that class hides `pre:has(> code.language-mermaid)`, so
  mermaid fences are never painted when JavaScript is on. Without
  JavaScript the class is never added and the source stays visible.
- `<mermaid-diagram>` gets a `min-height`, and a `modulepreload` link
  starts fetching Mermaid.js from the CDN before the component asks for
  it, so the reserved box fills in sooner.

Note that plain strings passed to `STYLE` and `SCRIPT` are HTML-escaped
like any other child, which would turn `'` into `&#x27;` and break a `>`
combinator. Raw CSS and JS are passed as `{__html: ...}`.

## Files

```
serve.nu                   -- the server
content/page.md            -- the markdown source
assets/mermaid-fences.js   -- swaps mermaid fences for <mermaid-diagram>
../mermaid-editor/assets/mermaid-diagram.js -- the web component (shared)
```

## What this example shows

- **`.md`** -- markdown to HTML with server-side highlighting of code
  fences, and how to handle a language the highlighter does not know.
- **`.highlight theme`** -- emitting a highlighter theme as CSS.
- **`.static`** -- serving assets from more than one directory, here to
  reuse a component from a sibling example.
- **Progressive enhancement** -- without JavaScript the page still shows
  the diagram source in a `<pre>`.
