# Change request: render `research/` as an in-game browsable section

**Audience:** the agent responsible for `examples/2048/`. This document
describes the work we want done; it is not the work itself.

## Goal

Add a "what is 2048?" link to the 2048 example's UI that opens a small
browsable site rendering the markdown files in this directory as
styled pages. The research material should be a first-class part of
the example, not a side note in a GitHub-only `.md` file.

## What exists already

`examples/2048/research/` contains two finished sections plus this
brief:

- `history/` -- six chapters on where 2048 came from (Threes!, 1024,
  Cirulli's weekend, the clones controversy, legacy). Plus
  `README.md` and `sources.md` inside that directory.
- `psychology/` -- four chapters on why 2048 is hard to put down
  (dopamine account, behavioral design, flow / friction, where the
  evidence thins). Plus `README.md` and `sources.md`.

The markdown files are the source of truth. The rendering work should
consume them unchanged; do not duplicate content into the rendering
code, and do not edit the markdown to accommodate rendering.

More sections may be added later under `research/<topic>/` -- the
routing should accept that without code changes per section.

## Reference implementation: `~/shapes/serve.nu`

`~/shapes/serve.nu` already implements the exact pattern we want. Lift
the recipe; don't try to re-derive it.

The relevant slices, by line:

- **`serve.nu:605-644`** -- `GET /docs/:slug`. Reads a markdown file,
  pipes through the `.md` command (built into http-nu, see
  `~/shapes/docs/templates.md`), wraps the resulting `{__html: ...}`
  record in `(ARTICLE $content)` inside a `MAIN > ASIDE + ARTICLE`
  layout. The aside is a per-chapter sidebar with an `active` class on
  the current page.
- **`serve.nu:573-602`** -- `GET /docs`. An index page; just a `UL`
  of links to each slug with a short description span.
- **`serve.nu:48-51`** -- `inject-copy-btns`. Tiny string-replace
  pass that wraps every `<pre>` the markdown renderer emits in a
  `<div class="code-block">` plus a button. Worth lifting verbatim,
  even though the research prose has few code blocks -- `sources.md`
  has none, but future research bundles may.
- **`serve.nu:53-235`** -- `page-head`. The styling. **Most of the
  article-typography CSS we want is in here**, in particular the
  `article h1` / `article h2` / `article p` / `article pre` /
  `article blockquote` / `article table` rules around lines 130-150,
  and the syntax-highlighting `article pre .keyword` ... block
  around lines 163-187. View-transition wiring is at 158-162.

The `.md` command produces a record of shape `{__html: "<...>"}`. The
http-nu HTML DSL knows to embed `__html` records as raw HTML, so the
output drops into `(ARTICLE ...)` cleanly.

## What to skip from shapes

`~/shapes` loads Stellar CSS from `http://localhost:7331/assets/css/stellar`
-- an external dev service. **Do not depend on it.** The 2048 example
must remain runnable with just `http-nu --datastar --services --store
./store :3002 examples/2048/serve.nu`; no second process.

Two options for replacing it:

1. Inline a small stylesheet that defines the CSS variables shapes'
   `article` rules reference (`--font-sans`, `--font-mono`,
   `--neutral-1` ... `--neutral-4`, `--font-size-{-2..5}`,
   `--size-{-2..3}`, `--border-radius-{-2..2}`, `--border-width-1`,
   `--code-bg`, `--code-fg`, etc.) with values that fit the existing
   2048 palette (the cream `#faf8ef` / wood-brown `#bbada0` / text
   `#776e65` in `static/styles.css`).
2. Rewrite the article CSS without variables. Less reusable, less
   work.

Either is fine. The constraint is that the about pages must feel like
part of the same site, not like a docs portal bolted on.

## Routes to add

In `examples/2048/serve.nu`:

- `GET /about` -- top-level index. Lists the available research
  sections (currently `history/` and `psychology/`) with a one-line
  description per section.
- `GET /about/:section` -- per-section index. Renders that section's
  `README.md` as the body, with the sidebar listing every chapter in
  the section in reading order.
- `GET /about/:section/:slug` -- per-chapter page. `slug` maps to
  `research/<section>/<slug>.md`. Use the chapter filenames as slugs
  verbatim (`01-threes`, `01-the-dopamine-account`, etc.). The
  `sources.md` of each section should be served as `sources`.
- 404 for unknown section or slug, same as shapes does.

**Section discovery should be dynamic.** Read `research/` at startup
(or per request) and enumerate the subdirectories. Do not hardcode
"history" and "psychology" -- a third section added later should work
without code changes.

**Chapter ordering inside a section** is by filename (the leading
`NN-` prefix sorts correctly). `README.md` is the section landing
page, not a chapter. `sources.md` should go at the end of the
sidebar.

**Display titles** should be derived from the first `# heading` of
each markdown file. Do not maintain a parallel title list in
`serve.nu`. The history `README.md` calls the section
"2048: a short history"; the psychology `README.md` calls it "2048:
why it feels dangerously addictive" -- these are the strings the
top-level `/about` index should show.

## Where to link in

Two places in the existing 2048 UI:

- **Splash (`GET /`)** -- next to or beneath the existing
  "+ New game" link, add "what is 2048?" pointing at `/about`.
  Splash is at `serve.nu:370`.
- **Play view (`GET /play/:game_id`)** -- in the existing hint
  paragraph that already contains the "all games" link
  (`serve.nu:439-448`), append "what is 2048?". Keep the link styling
  consistent with the other links there.

Both links should respect `$req | href` so they work behind any mount
prefix. Look at how `/new` and `/` are already linked for the pattern.

## Markdown source ergonomics

A few things to be aware of when wiring `.md` to these files:

- **Intra-section relative links** -- e.g.
  `[02-1024.md](02-1024.md)` from `history/01-threes.md`. Rewrite to
  `/about/history/02-1024` in a post-render string-replace pass
  alongside `inject-copy-btns`. Keeps the markdown usable as plain
  files on GitHub *and* working in the rendered site.
- **Cross-section relative links** -- e.g.
  `[../history/](../history/)` from `psychology/README.md`. Rewrite
  to `/about/history`.
- **External absolute links** (URLs in `sources.md`) should stay
  intact and get `target="_blank" rel="noopener"`.
- Both `README.md` files have a "## Reading order" section that
  duplicates the sidebar. That is fine; leave it. The sidebar is the
  navigation, the README is the prose introduction.

## Trade-offs we have already decided

- **Self-contained over pretty-out-of-the-box.** No Stellar
  dependency.
- **Render markdown server-side, no client-side markdown lib.**
  `.md` is built in; use it.
- **No new directories under `examples/2048/`.** The about pages
  share `serve.nu` and the existing `static/` directory. Don't
  introduce a separate site or a build step.
- **No edits to the markdown for rendering purposes.** Any quirk of
  rendering is solved in the renderer, not by mutating the source.
- **Sections are dynamic.** Sections discovered from filesystem
  layout, not hardcoded.

## Out of scope

- Search across the chapters.
- A table of contents per chapter.
- Edit links to GitHub.
- Anything responsive beyond what shapes already does (the
  `@media (max-width: 768px)` at the bottom of shapes' CSS stacks
  the sidebar above the article; that's enough).

## Acceptance

- `http-nu --datastar --services --store ./store :3002
  examples/2048/serve.nu` still works with no extra services.
- `GET /about` shows a section index with `history` and `psychology`.
- `GET /about/history` renders the history README with a sidebar
  listing the six chapters + sources.
- `GET /about/psychology` renders the psychology README with a
  sidebar listing the four chapters + sources.
- Adding a new section directory `research/<new>/` with a
  `README.md` and some `NN-*.md` files would appear in `/about` and
  be browsable without serve.nu changes.
- Clicking any sidebar entry navigates to a styled chapter page; the
  active item is highlighted.
- Intra- and cross-section links work; external links open without
  losing context.
- The play view and splash both link into `/about`.
- No edits to any `research/**/*.md` file.
