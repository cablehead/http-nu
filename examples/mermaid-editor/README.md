# Live Mermaid Editor

A live diagram editor in 80 lines of Nushell. Type
[Mermaid](https://mermaid.js.org/) syntax on the left, see the rendered diagram
on the right.

This example demonstrates how
[http-nu](https://github.com/cablehead/http-nu) and
[Datastar](https://data-star.dev) work together with a vanilla web component --
no build step, no framework, no client-side state management.

## Run

```bash
http-nu --datastar :3001 serve.nu
```

Open http://localhost:3001.

## How it works

The entire app is one request/response cycle:

1. **Page load** -- `GET /` serves the editor UI using http-nu's HTML DSL. The
   textarea gets a default Mermaid diagram. A `<mermaid-diagram>` web component
   renders it immediately.

2. **User types** -- Datastar's `data-bind:source` syncs the textarea value into
   a reactive signal. After a 500ms debounce, `@post('/')` sends it to the
   server.

3. **Server responds** -- The POST handler parses the signal, wraps the source in
   a `<mermaid-diagram>` element, and returns it as a Datastar
   `patch-elements` SSE event.

4. **Diagram updates** -- Datastar morphs the `<mermaid-diagram>` element in the
   DOM. The web component's MutationObserver detects the text change and
   re-renders the diagram via Mermaid.js.

```
textarea  ──data-bind:source──>  $source signal
                                      │
                          data-on:input (500ms debounce)
                                      │
                                 @post('/')
                                      │
                              ┌───────▼───────┐
                              │   http-nu     │
                              │  serve.nu     │
                              └───────┬───────┘
                                      │
                           to datastar-patch-elements
                                      │
                              SSE: patch <mermaid-diagram>
                                      │
                              Datastar morphs DOM
                                      │
                          MutationObserver fires
                                      │
                              Mermaid re-renders
```

## The web component

`<mermaid-diagram>` is a standalone web component (~120 lines) with no
dependencies beyond Mermaid.js itself. It renders Mermaid diagrams from its text
content and re-renders when that content changes.

```html
<mermaid-diagram>
  graph TD
    A --> B
</mermaid-diagram>
```

**Why it plays well with Datastar (and any morpher):**

- **Shadow DOM** separates concerns -- the source text lives in the light DOM
  (where morphers operate), the rendered SVG lives in the shadow DOM (where it
  won't interfere).
- **MutationObserver** watches the light DOM for changes. When a morpher patches
  the text content, the observer fires and triggers a re-render.
- **Render queue** serializes all renders to prevent race conditions when
  multiple instances or rapid updates occur.
- **Lazy loading** -- Mermaid.js is imported on first use, not at page load.

## Files

```
serve.nu                  -- the server (80 lines of Nushell)
assets/mermaid-diagram.js -- the web component (~120 lines)
```

## What this example shows

- **http-nu's HTML DSL** -- build pages with `HTML`, `HEAD`, `BODY`, `DIV`,
  `TEXTAREA`, etc. Plain strings are auto-escaped.
- **http-nu's Datastar SDK** -- `from datastar-signals`, `to
  datastar-patch-elements`, `to sse` for the full request/response cycle.
- **Datastar attributes** -- `data-bind:source` for two-way binding,
  `data-on:input__debounce.500ms` for throttled server calls.
- **Web components + morphing** -- how to build components that survive DOM
  patching. The pattern (content in light DOM, output in shadow DOM,
  MutationObserver for reactivity) works with Datastar, htmx/idiomorph, or any
  morpher.
- **Zero client JS** -- beyond the web component and Datastar itself, there is
  no application JavaScript. The server drives everything.
