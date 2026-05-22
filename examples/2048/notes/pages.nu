# Pure helpers for the notes sub-site: slug + split a markdown file on
# h1 boundaries. Kept separate from serve.nu so test.nu can `use` them
# (serve.nu's trailing route closure makes it ineligible as a module).

# Slugify a heading: lowercase, non-alphanumeric runs become single
# hyphens, trim leading/trailing hyphens.
export def slugify [s: string]: nothing -> string {
  $s | str downcase | str replace -ar '[^a-z0-9]+' '-' | str trim --char '-'
}

# Split a markdown file on h1 boundaries. Returns [{slug, title, body}].
# `open --raw` -- bare `open` on a .md file parses it into a structured
# table (the commonmark AST), which then errors at `split row`. The
# regex match-mode split on `^# ` handles files whether or not they
# start with the first heading (no preamble assumption).
export def split-md [path: string]: nothing -> list {
  open --raw $path | decode utf-8
  | str trim
  | split row -r '(?m)^# '
  | where {|s| ($s | str length) > 0 }
  | each {|sec|
      let lines = $sec | lines
      let title = $lines | first
      {
        slug: (slugify $title)
        title: $title
        body: ($lines | skip 1 | str join "\n")
      }
    }
}
