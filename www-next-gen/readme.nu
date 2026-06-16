# readme.nu - split one large markdown document (a README) into navigable
# doc pages, driven by nushell's `from md --verbose` AST instead of
# hand-maintained section files.
#
# The AST gives every heading's level and exact source line, so pages are
# sliced out of the raw text by line range: code fences can never be
# mistaken for headings, and the README stays the single source of truth.

# GitHub-style anchor slug for a heading title.
export def slugify []: string -> string {
  $in
  | str downcase
  | str replace -ra '[^a-z0-9 -]' ''
  | str replace --all ' ' '-'
}

# Collect the plain text of an AST node (headings contain nested link /
# code_inline / text children).
def node-text []: record -> string {
  let node = $in
  let own = ($node.attrs?.value? | default "")
  let kids = ($node.children? | default [] | each { $in | node-text } | str join "")
  $own + $kids
}

# All headings in the document: {level, line, title, slug}.
export def headings []: string -> table {
  $in
  | from md --verbose
  | where type =~ '^h[1-6]$'
  | each {|h|
      let title = ($h | node-text | str trim)
      {
        level: $h.attrs.level
        line: $h.position.start.line
        title: $title
        slug: ($title | slugify)
      }
    }
}

# Split the document into pages.
#
# $split lists heading slugs that act as groups: their child headings
# become pages of their own (recursively), instead of being folded into
# one page. Everything before the first heading-page (logo, badges, toc)
# is skipped.
#
# Returns: {slug, title, level, start, end, group}
#   start/end are 1-based source line ranges, group is the slug of the
#   enclosing group heading or null.
export def pages [split: list<string>]: string -> table {
  let text = $in
  let total = ($text | lines | length)
  let hs = ($text | headings)

  let result = ($hs | enumerate | reduce --fold {stack: [], pages: []} {|it, acc|
    let h = $it.item
    let idx = $it.index

    # pop ancestors at the same or deeper level
    let stack = ($acc.stack | where level < $h.level)
    let ancestors_are_groups = ($stack | all {|a| $a.is_group })
    let is_group = ($h.slug in $split)

    # section end: line before the next heading at the same or higher level
    let next = ($hs | skip ($idx + 1) | where level <= $h.level | first | default null)
    let section_end = (if $next == null { $total } else { $next.line - 1 })

    let pages = (if (not $ancestors_are_groups) {
      $acc.pages
    } else if $is_group {
      # group intro: lines between the group heading and its first child
      let child = ($hs | skip ($idx + 1) | where line <= $section_end | first | default null)
      let intro_end = (if $child == null { $section_end } else { $child.line - 1 })
      let has_body = (
        $text | lines | slice $h.line..($intro_end - 1) | any {|l| ($l | str trim) != "" }
      )
      if $has_body {
        $acc.pages | append {
          slug: $h.slug, title: $h.title, level: $h.level
          start: $h.line, end: $intro_end
          group: ($stack | last | get slug? | default null)
        }
      } else {
        $acc.pages
      }
    } else {
      $acc.pages | append {
        slug: $h.slug, title: $h.title, level: $h.level
        start: $h.line, end: $section_end
        group: ($stack | last | get slug? | default null)
      }
    })

    {
      stack: ($stack | append {level: $h.level, is_group: $is_group, slug: $h.slug})
      pages: $pages
    }
  })

  $result.pages
}

# Map every heading anchor to the page that renders it.
export def anchor-map [split: list<string>]: string -> record {
  let text = $in
  let pgs = ($text | pages $split)
  $text
  | headings
  | reduce --fold {} {|h, acc|
      let owner = ($pgs | where {|p| $h.line >= $p.start and $h.line <= $p.end } | first | default null)
      # duplicate heading titles: first occurrence keeps the plain slug,
      # matching GitHub's anchor scheme
      if $owner == null or ($h.slug in $acc) { $acc } else { $acc | insert $h.slug $owner.slug }
    }
}

# Rewrite the href attributes of already-rendered HTML:
#   #anchor          -> /base/<owning-page>#anchor (or left alone if the
#                       anchor is on this same page)
#   relative/path    -> $repo/relative/path (links into the source tree)
# Absolute http(s):// links and mailto: are left untouched.
def rewrite-links [anchors: record base: string repo: string]: string -> string {
  $in | str replace -ra 'href="([^"]*)"' {|href|
    let new = if ($href | str starts-with "#") {
      let slug = ($href | str substring 1..)
      let pg = ($anchors | get -o $slug)
      if $pg == null { $href } else { $"($base)/($pg)#($slug)" }
    } else if ($href =~ '^[a-z][a-z0-9+.-]*:') or ($href | str starts-with "/") or ($href | str starts-with "//") {
      $href
    } else {
      $"($repo)/($href)"
    }
    $'href="($new)"'
  }
}

# Render one page to {__html}. Headings are rebased so the page heading
# becomes <h1>, ids are injected for anchors, intra-document links are
# rewritten to their owning page, and relative file links point at $repo.
export def render-page [
  page: record
  anchors: record  # anchor slug -> page slug
  --base: string = "/docs"
  --repo: string = "https://github.com/cablehead/http-nu/blob/main"
]: string -> record {
  let text = $in
  let lines = ($text | lines)
  let hs = ($text | headings | where line >= $page.start and line <= $page.end)
  let shift = ($page.level - 1)

  let html = ($hs | enumerate | each {|it|
    let h = $it.item
    let seg_end = ($hs | get -o ($it.index + 1) | get -o line | default ($page.end + 1)) - 1
    let level = ([($h.level - $shift) 6] | math min)
    let hashes = (1..$level | each { "#" } | str join "")
    let heading_md = ($hashes + " " + ($lines | get ($h.line - 1) | str replace -ra '^#+\s*' ''))
    let body_md = ($lines | slice $h.line..($seg_end - 1) | str join "\n")

    # render markdown, then post-process the resulting HTML: give the
    # heading its anchor id and rewrite links by attribute (robust against
    # code fences, html blocks, etc.)
    ($heading_md + "\n" + $body_md)
    | .md | get __html
    | str replace $"<h($level)>" $"<h($level) id=\"($h.slug)\">"
    | rewrite-links $anchors $base $repo
  } | str join "\n")

  {__html: $html}
}
