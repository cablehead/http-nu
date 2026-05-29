# Inline `source X.nu` directives into a single self-contained Nu script.
#
# Why: Nu's `source` resolves file paths via `std::fs` at PARSE time. On
# wasm32 (Cloudflare Workers) there's no fs, so the hub's
# `let basic = source basic.nu` fails with `SourcedFileNotFound`. The CF
# build embeds one file via `include_str!`; this script pre-resolves all
# its `source` references into one inlined script so embedding suffices.
#
# Substitution rule (textual, single-pass per file, recursive on children):
#   let NAME = source REL.nu     ->     let NAME = do {
#                                          <contents of REL.nu>
#                                        }
#
# Limitations:
# - Only `let NAME = source REL` is recognized; bare `source X` (where the
#   caller depends on `source`'s scope-merging semantics rather than its
#   return value) is left alone. The hub doesn't use that form.
# - REL is resolved relative to the file being processed, matching `source`.
# - Tabs vs spaces in indentation are preserved.
#
# Usage:
#   nu scripts/bundle-cf-handler.nu examples/serve.nu > target/cf-hub.nu

def bundle [src: string] {
    let dir = ($src | path dirname)
    open --raw $src
    | lines
    | each {|line|
        let cap = ($line | parse --regex '^(?<indent>\s*)let\s+(?<name>\S+)\s*=\s*source\s+(?<rel>\S+\.nu)\s*$')
        if ($cap | is-empty) {
            $line
        } else {
            let m = ($cap | first)
            let subpath = ($dir | path join $m.rel)
            let inlined = (bundle $subpath)
            $"($m.indent)let ($m.name) = do {(char nl)($inlined)(char nl)($m.indent)}"
        }
    }
    | str join (char nl)
}

def main [src: string] {
    print -n (bundle $src)
}
