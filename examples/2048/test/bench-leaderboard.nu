# Bench `top-players`: seeds a synthetic store at several scales and
# times the leaderboard computation. Each scale spins a fresh ephemeral
# store so previous-run frames don't bleed into the timing.
#
# Run:
#   ./examples/2048/test/bench-leaderboard.sh
#
# (The script wrapper boots http-nu eval per scale with --store pointing
# at a temp dir; the .nu body below assumes it's already running inside
# such an instance.)

use ../tfe/store.nu *

# Scales: (players, games per player, snapshots per game).
# Snapshot count per game is the dominant cost driver since `top-players`
# does one `.last` per game (xs has to find the topic head).
const SCALES = [
  [10  2  50]
  [50  5  100]
  [100 10 200]
]

# Seed the configured store with synthetic frames. Players are
# "bench-player-<n>"; games are appended onto `player.<id>.games` and
# the returned frame's id becomes the game_id used in snapshot topics.
# Snapshot metas carry only what `top-players` reads (state is dropped
# to keep frame size sane at high scales).
def seed [players: int, games_per_player: int, snapshots_per_game: int] {
  for p in 0..($players - 1) {
    let player_id = $"bench-player-($p)"
    for g in 0..($games_per_player - 1) {
      let game_frame = null | .append $"player.($player_id).games"
      let game_id = $game_frame.id
      # Synthetic per-game peak score so the leaderboard has variety
      # across players (top players are determined by their best game).
      let peak = ($p * 1000 + $g * 50)
      for i in 1..$snapshots_per_game {
        null | .append $"game.snapshot.($game_id)" --meta {
          state: {}
          player_id: $player_id
          last_move_id: ""
          score: ($i * 10 + $peak)
          max_tile: 128
          moves: $i
          game_over: ($i == $snapshots_per_game)
        }
      }
    }
  }
}

let scale = $env.BENCH_SCALE? | default "0" | into int
let cfg = $SCALES | get $scale
let players = $cfg | get 0
let games_per_player = $cfg | get 1
let snapshots_per_game = $cfg | get 2
let total_games = $players * $games_per_player
let total_frames = $total_games * (1 + $snapshots_per_game)

print $"scale: ($players) players x ($games_per_player) games x ($snapshots_per_game) snapshots/game"
print $"  total games: ($total_games), total appended frames: ($total_frames)"

let seed_time = timeit { seed $players $games_per_player $snapshots_per_game }
print $"  seed time: ($seed_time)"

# Warm + measure. First call may pay extra cost (caches / mmap pages),
# so we time three back-to-back and report the median-ish middle one.
let t1 = timeit { top-players --limit 10 | length }
let t2 = timeit { top-players --limit 10 | length }
let t3 = timeit { top-players --limit 10 | length }
print $"  top-players: ($t1), ($t2), ($t3)"
