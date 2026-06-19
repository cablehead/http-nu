# Render a README as a doc site

Your `README.md` is already the story of your project. http-nu turns it into a
documentation site as rich as [Astro Starlight](https://starlight.astro.build),
with much less fuss. The whole job is two pieces: `.md`, which renders Markdown to
HTML with syntax-highlighted code blocks, and a handful of lines of Nushell to
route it.

This page is one of those Markdown files, rendered exactly the way it describes.

## The smallest doc site

One route, the README rendered to HTML:

```nushell
const readme = (open --raw README.md | decode utf-8)

{|req| $readme | .md }
```

```bash
http-nu :3001 serve.nu
```

Open it and you have a readable page: headings, lists, links, and code blocks
colored by `.md`, which highlights `bash`, `nushell`, `rust`, and more via
TextMate grammars. No build step, no `node_modules`.

## Split it into pages

A long README scrolls forever. Break it into navigable pages by heading, and
route each one by its slug:

```nushell
const readme = (open --raw README.md | decode utf-8)

# one page per "## " section
let pages = ($readme | split row "\n## " | skip 1 | each {|s|
  let title = ($s | lines | first)
  { slug: ($title | str downcase | str replace --all " " "-")
    body: $"## ($s)" }
})

{|req|
  let slug = ($req.path | str trim --left --char "/")
  let page = ($pages | where slug == $slug | get 0?)
  if $page == null { "not found" } else { $page.body | .md }
}
```

Add a sidebar that lists the sections and you have navigation. The README stays
the single source of truth; you just slice it.

> **A living example.** The [Reference](/reference) section of this site is the
> code above, running: the http-nu README, sliced one page per `## ` heading,
> with a sidebar of slugs and a prev/next pager. Click through it, then come
> back here, the code reads like a description of what you just used.

## Typography

`.md` gives you semantic HTML, so style raw tags once and every page looks good
with no per-page classes. This site uses stellar tokens plus a small base layer,
but bring whatever CSS you like.

## Beyond Markdown

When a page is rendered from data rather than static prose, reach for `.mj`
(minijinja templates). You can poke at both `.md` and `.mj` in the
[Templates theme](/themes/templates).

## The result

This site. The [Reference](/reference) is this project's own README, rendered;
the themes wrap interactive toys around it. A README, a few lines of Nushell, and
you are done.
