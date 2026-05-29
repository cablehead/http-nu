# Migrate an xs store from the old 2048 topic shape to the new one:
#
#   game.<id>.move      ->  game.move.<id>
#   game.<id>.snapshot  ->  game.snapshot.<id>
#
# Works on an export dump (the format xs's `.export` writes and
# `.import` reads -- `frames.jsonl` + `cas/`). CAS blobs are copied
# byte-for-byte; only the `topic` field of each frame is rewritten.
# Frame ids, meta, hash, ttl, and ordering are preserved.
#
# Usage (against a live xs server hosting the old store):
#
#   $env.XS_ADDR = (realpath ./store-old)
#   use /path/to/xs/xs.nu *
#   .export /tmp/dump-old
#
#   nu examples/2048/migrate-topics.nu /tmp/dump-old /tmp/dump-new
#
#   $env.XS_ADDR = (realpath ./store-new)
#   .import /tmp/dump-new
#
# (The new store can be a fresh `xs serve` instance or any xs-compatible
# server -- we only need its `/import` endpoint reachable via XS_ADDR.)

def main [src: string, dst: string] {
  if not ($src | path exists) { error make {msg: $"src does not exist: ($src)"} }
  if ($dst | path exists)     { error make {msg: $"dst already exists: ($dst)"} }

  mkdir $dst
  mkdir ($dst | path join "cas")

  # CAS blobs are content-addressed -- copy unchanged.
  let src_cas = ($src | path join "cas")
  if ($src_cas | path exists) {
    ls $src_cas | each {|f|
      cp $f.name ($dst | path join "cas" | path join ($f.name | path basename))
    } | ignore
  }

  # frames.jsonl: one JSON frame per line. Rewrite topic and re-serialize,
  # leaving everything else (id, meta, hash, ttl) untouched.
  let src_jsonl = ($src | path join "frames.jsonl")
  let dst_jsonl = ($dst | path join "frames.jsonl")
  open --raw $src_jsonl
  | lines
  | each {|line|
      $line
      | from json
      | update topic {|f| $f.topic
          | str replace --regex '^game\.([a-z0-9]+)\.move$'     'game.move.$1'
          | str replace --regex '^game\.([a-z0-9]+)\.snapshot$' 'game.snapshot.$1'
        }
      | to json --raw
    }
  | str join "\n"
  | save --raw $dst_jsonl

  # Brief tally: total frames, and how many topics now match the new shape.
  let lines = (open --raw $dst_jsonl | lines)
  let total = ($lines | length)
  let renamed = (
    $lines | where {|l|
      let t = ($l | from json | get topic)
      ($t | str starts-with "game.move.") or ($t | str starts-with "game.snapshot.")
    } | length
  )
  print $"migrated: ($src_jsonl) -> ($dst_jsonl)  | frames: ($total)  | renamed topics: ($renamed)"
}
