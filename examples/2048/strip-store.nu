# Strip a 2048 xs store down to just the raw played game data, normalized
# to the current topic shape. Operates on a `frames.jsonl` dump (one JSON
# frame per line, the format `xs cat` writes).
#
# KEEPS, rewriting the old topic shape to the new one:
#
#   player.<uuid>.games                      game creation; frame id = game id
#   game.<id>.move      ->  game.move.<id>   a player's move (old shape)
#   game.move.<id>                           a player's move (already current)
#
# Old-shape move frames carry only `{intent, req_id}` in meta -- they
# predate the `user_id` stamp. The current snapshot-actor gates each move
# on `move-authorized` (meta.user_id == the game's owner), so a move with
# no user_id is dropped and the game never rebuilds. The owner is
# unambiguous (the game id IS its player.<uuid>.games frame id), so this
# script backfills `user_id` into any move that lacks one, bringing the
# move meta up to the current schema.
#
# DROPS everything else. None of it is raw game data -- it is either
# derived state or runtime-managed plumbing, and it comes back on its own
# when serve.nu starts:
#
#   game.snapshot.<id> / game.<id>.snapshot
#       Derived. The snapshot-actor rebuilds the full chain (root + every
#       move, with prev links) from the move log on its first boot against
#       a snapshot-less store -- see snapshot-actor.nu's `start: "first"`.
#   <name>.register / .active / .unregistered, game.nu
#       Pre-rename xs lifecycle + module frames. The 0.13 runtime ignores
#       them (ADR 0005); serve.nu re-registers under xs.actor.* /
#       xs.module.* at startup.
#   page.html / base.html / nav.html, session.*, xs.start, bus.*,
#   leaderboard.top, _presence.*, audit.*
#       Page chrome, auth sessions, server-boot markers, and ephemeral
#       UI/derived frames.
#
# Frame ids, meta, hash, and ttl are preserved verbatim; only old-shape
# move topics are rewritten. Kept frames carry no CAS, so there is no
# cas/ to copy.
#
# Workflow (source and destination are vanilla `xs serve` stores; the
# game id is the player.<uuid>.games frame id, so ids MUST be preserved --
# use `xs import`, never `xs append`):
#
#   xs serve ./store-old &                  # source
#   xs cat ./store-old/sock | save raw.jsonl
#
#   nu strip-store.nu raw.jsonl stripped.jsonl
#
#   xs serve ./store-new &                  # fresh, empty
#   open stripped.jsonl
#   | lines
#   | each {|l| $l | xs import ./store-new/sock }
#   | ignore
#
#   http-nu --services --store ./store-new :3002 examples/2048/serve.nu
#   # The snapshot-actor finds no snapshots, so `start: "first"` replays
#   # the move log once and rebuilds every game from scratch.

def is-move [t: string] {
  ($t | str starts-with "game.move.") or (($t | str starts-with "game.") and ($t | str ends-with ".move"))
}

def is-games [t: string] {
  ($t | str starts-with "player.") and ($t | str ends-with ".games")
}

def main [src: string, dst: string] {
  if not ($src | path exists) { error make {msg: $"src does not exist: ($src)"} }
  if ($dst | path exists)     { error make {msg: $"dst already exists: ($dst)"} }

  let frames = (
    open --raw $src
    | lines
    | where {|l| ($l | str trim) != "" }
    | each {|l| $l | from json }
  )
  let total = ($frames | length)

  # game id (= player.<uuid>.games frame id) -> owner uuid, used to
  # backfill user_id into pre-stamp move frames.
  let owners = (
    $frames
    | where {|fr| is-games $fr.topic }
    | reduce --fold {} {|fr acc|
        $acc | upsert $fr.id ($fr.topic | str replace "player." "" | str replace ".games" "")
      }
  )

  # Keep raw game data only; rewrite old-shape move topics to new shape
  # and backfill any missing user_id from the game's owner.
  let kept = (
    $frames
    | where {|fr| (is-move $fr.topic) or (is-games $fr.topic) }
    | each {|fr|
        if (is-games $fr.topic) { return $fr }
        let topic = ($fr.topic | str replace --regex '^game\.([a-z0-9]+)\.move$' 'game.move.$1')
        let gid = ($topic | str replace "game.move." "")
        let uid = ($fr.meta | get user_id? | default "")
        let meta = if ($uid | is-empty) {
          $fr.meta | default {} | upsert user_id ($owners | get --optional $gid | default "")
        } else {
          $fr.meta
        }
        $fr | update topic $topic | update meta $meta
      }
  )

  # Trailing newline so line-at-a-time readers (e.g. shell `while read`)
  # don't drop the last frame; nushell `lines` is fine either way.
  ($kept | each {|fr| $fr | to json --raw } | str join "\n") + "\n" | save --raw $dst

  let games = ($kept | where {|fr| is-games $fr.topic } | length)
  let moves = ($kept | where {|fr| is-move $fr.topic } | length)
  # backfilled: source moves that arrived with no user_id (counted before
  # enrichment, so the topic is still in either shape).
  let backfilled = (
    $frames
    | where {|fr| is-move $fr.topic }
    | where {|fr| ($fr.meta | get user_id? | default "") == "" }
    | length
  )
  let orphans = (
    $kept
    | where {|fr| is-move $fr.topic }
    | where {|fr| ($owners | get --optional ($fr.topic | str replace "game.move." "")) == null }
    | length
  )
  let kn = ($kept | length)
  print $"read ($total) frames  ->  kept ($kn)  dropped (($total - $kn))"
  print $"  player.<uuid>.games          : ($games)"
  print $"  game.move.<id>               : ($moves)"
  print $"  moves backfilled with user_id: ($backfilled)"
  print $"  orphan moves \(no owning game\): ($orphans)"
}
