use http-nu/router *
use http-nu/datastar *
use http-nu/html *
use http-nu/http *

# The tfe/ module is split into submodules: game (pure logic), render
# (HTML output), sse (server pipeline), store (.cat/.last helpers).
# render.nu's `export-env` compiles the board template once -- and
# export-env only fires on direct `use module/sub.nu`, not via
# `export use` in a parent mod.nu -- so we import each submodule here
# rather than going through tfe/mod.nu.
use ./tfe/game.nu *
use ./tfe/render.nu *
use ./tfe/sse.nu *
use ./tfe/store.nu *
use ./auth.nu *

const SCRIPT_DIR = path self | path dirname
const STATIC_DIR = $SCRIPT_DIR | path join "static"
# Cache-buster for static assets: fresh per server start, stable within one
# session. Browsers cache /styles.css?v=<REV> across page loads but refetch
# on the next server restart.
let REV = random uuid | str substring 0..7

# Splash board replays this real game's snapshot stream on loop. Each
# visit picks a random starting frame so two simultaneous viewers see
# different moves. See notes/in-nushell for why even the splash is real
# data (it is the SSE pipeline -- pointed at a stored game instead of
# a live one).
const SPLASH_GAME_ID = "03g561k2p2p4ftv9p9iykb1kf"
# Load the snapshot stream ONCE at server start (closures inherit this
# binding). 2880-ish frames, all in memory, indexable in O(1).
let SPLASH_STATES = if ($HTTP_NU.store? | default null) == null { [] } else {
  try { .cat -T $"game.($SPLASH_GAME_ID).snapshot" | get meta.state } catch { [] }
}
# YYYY-MM-DD of the run (SCRU128 timestamp 1779014675.541 = 2026-05-17).
# Hardcoded for now -- `.id unpack` isn't in scope at module-init time;
# upstream fix pending. Re-derive when the splash game changes.
let SPLASH_DATE = "2026-05-17"
# Player id whose game we are replaying. Used to deep-link the credit
# to /by/<id> so visitors can see oleksii_lisovyi's other games.
const SPLASH_PLAYER_ID = "542221d8-be77-4fac-91cb-1bfa49ae3b2a"

# Register the xs snapshot-actor (singleton): it watches every
# `player.*.games` + `game.*.move` frame and writes the canonical
# `game.<id>.snapshot` (ttl: last:1). Requires `--store` + `--services`;
# guarded so that test.nu, which sources serve.nu without a store, stays
# happy. Re-registering on each startup replaces the running actor (per
# xs's `<name>.register` semantics), so this is restart-safe.
if ($HTTP_NU.store? | default null) != null and ($HTTP_NU.services? | default false) {
  open ($SCRIPT_DIR | path join "tfe" "game.nu") | .append game.nu
  open ($SCRIPT_DIR | path join "tfe" "snapshot-actor.nu") | .append snapshot-actor.register
}

# Render a card from a games_topic frame (the initial page render). Resumes
# the game to get state, then defers to render-card-from-state.
# Caller chooses the destination via `--href`; defaults to /play.
def render-game-card [req: record game_frame: record --href: string]: nothing -> record {
  let resumed = (resume-game $game_frame.id)
  let h = if ($href | is-empty) { ($req | href $"/play/($game_frame.id)") } else { $href }
  render-card-from-state $req $game_frame.id $resumed.state $resumed.moves $resumed.follow_from_id --href $h
}

# --- sub-handlers ---------------------------------------------------------

# The /notes digital-garden sub-site. One file per topic in
# notes/content/*.md; each h1 becomes its own page at runtime.
let notes = source notes/serve.nu

# The /design component viewer. 2-col sidebar + focused preview; Ctrl-N/P
# navigate the catalog.
let design = source design/serve.nu

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
    (mount "/notes" $notes)
    (mount "/design" $design)
    (route {method: POST path: "/move"} {|req ctx|
      # The client carries the game id (URL-routed play view, so the
      # page knows which game it's on). Body shape: {gameId, intent,
      # reqId}. The server stamps the resolved user_id + session_id on
      # the frame meta -- the snapshot-actor compares user_id against
      # the game's owner and silently drops mismatches. Anonymous
      # requests (no session) are rejected at the HTTP layer.
      let signals = $in | from datastar-signals $req
      let game_id = $signals | get gameId? | default ""
      let intent = $signals | get intent? | default ""
      let req_id = $signals | get reqId? | default ""
      let session = (resolve-session $req)
      if ($game_id | is-empty) {
        null | metadata set { merge {'http.response': {status: 400}} }
      } else if $session == null {
        # The UI never generates an unauthenticated /move. Treat it as
        # malicious traffic: audit and 204 silently (probes get no
        # information). External actors can subscribe to
        # `audit.move.no-session` to alert / rate-limit.
        null | .append "audit.move.no-session" --ttl ephemeral --meta {
          game_id: $game_id
          intent: $intent
          remote_ip: ($req.remote_ip? | default "")
          trusted_ip: ($req.trusted_ip? | default "")
          user_agent: ($req.headers | get user-agent? | default "")
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      } else {
        let topic = $"game.($game_id).move"
        let meta_base = {
          user_id: $session.user_id
          session_id: $session.session_id
          req_id: $req_id
        }
        if $intent == "undo" {
          null | .append $topic --meta ($meta_base | upsert kind "undo")
        } else if $intent == "" {
          # RTT-measurement ping: client sends an empty intent so the SSE
          # echoes a no-op state. Don't persist -- ephemeral keeps the
          # move log clean.
          null | .append $topic --ttl ephemeral --meta ($meta_base | upsert intent $intent)
        } else {
          # Covers h/j/k/l.
          null | .append $topic --meta ($meta_base | upsert intent $intent)
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      }
    })

    (route {method: GET path: "/sse/splash"} {|req ctx|
      # Pure reader: subscribe to bus.splash.seek and emit a board +
      # pos-signal patch per frame. No sleep, no per-connection
      # generator state -- cadence lives in the client (a Datastar
      # `data-on:interval` on the slider posts $pos+1 every 1.2s; the
      # `data-on:input` on the same slider posts on drag). The bus is
      # the single source of truth; every viewer sees the same patches
      # because they all follow the same topic.
      #
      # `--last 1` flushes the current pos to a freshly-connected
      # viewer right away (no gap before they see a board).
      let states = $SPLASH_STATES
      if ($states | is-empty) {
        null | metadata set { merge {'http.response': {status: 204}} }
      } else {
        let n = $states | length
        .cat --last 1 --follow -T "bus.splash.seek"
        | where ($it.topic? | default "") == "bus.splash.seek"
        | each {|f|
            let pos = ((($f.meta? | default {} | get pos? | default 0) | into int) mod $n)
            let state = $states | get $pos
            # WC variant: ship the state as a signal; <game-board> picks
            # it up via data-attr:state and runs its own animation. The
            # counter is signal-bound on the client side (data-text on
            # $splashPos), so we just need to push the pos number here.
            # Strip per-tile animation hints (spawned / merged / ghosts)
            # from the wire payload; the WC diffs by id.
            let board = {
              tiles: ($state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
              gameOver: ($state | get game_over? | default false)
            }
            {splashState: $board, splashPos: $pos}
            | to datastar-patch-signals
          }
        | to sse
      }
    })

    (route {method: POST path: "/splash/seek"} {|req ctx|
      # The splash cadence -- both auto-tick (data-on:interval) and
      # user drag (data-on:input) -- posts here. Body: {pos: <int>}.
      # Append onto bus.splash.seek with last:1 (the cap retains only
      # the latest seek, but `.cat --follow` still delivers every
      # appended frame to active viewers, so this works as a pub).
      let signals = $in | from datastar-signals $req
      let pos = $signals | get pos? | default 0 | into int
      null | .append "bus.splash.seek" --ttl last:1 --meta {pos: $pos}
      null | metadata set { merge {'http.response': {status: 204}} }
    })

    (route {method: GET path: "/sse/games"} {|req ctx|
      # Live updates for the all-games splash.
      #
      # Strategy: keep an in-memory {game_id: snapshot_meta} record as
      # the generate accumulator. On a snapshot frame the meta itself
      # IS the new state -- just upsert. On a new games_topic frame
      # (player.<id>.games), pull the root snapshot the actor just
      # wrote and add it. Either path re-renders the whole .games-list
      # from $data and emits a single morph patch. morphdom diffs the
      # new HTML against the DOM so unchanged cards don't mutate.
      let session = (resolve-session $req)
      if $session == null {
        null | metadata set { merge {'http.response': {status: 401}} }
      } else {
        let player_id = $session.user_id
        let games_topic = $"player.($player_id).games"
        let head = (try { .cat | last | get id? } catch { null })
        # Seed $data with the player's existing games + their latest
        # snapshots so the first push is consistent with the initial
        # page render.
        let initial_data = (try { .cat -T $games_topic } catch { [] })
          | reduce -f {} {|f acc|
              let snap = try { .last $"game.($f.id).snapshot" } catch { null }
              if $snap == null { $acc } else { $acc | upsert $f.id $snap.meta }
            }
        .cat --follow
        | generate {|item data|
            if ('event' in $item) { return {out: $item, next: $data} }
            if ($head != null and $item.id <= $head) { return {next: $data} }
            let changed_id = if $item.topic == $games_topic {
              $item.id
            } else if (($item.topic | str ends-with ".snapshot") and (($item.meta? | get player_id? | default "") == $player_id)) {
              $item.topic | str replace "game." "" | str replace ".snapshot" ""
            } else { null }
            if $changed_id == null { return {next: $data} }
            let new_meta = if $item.topic == $games_topic {
              try { .last $"game.($item.id).snapshot" | get meta } catch { null }
            } else { $item.meta }
            if $new_meta == null { return {next: $data} }
            let is_new_card = ($changed_id not-in ($data | columns))
            let new_data = $data | upsert $changed_id $new_meta
            if $new_data == $data { return {next: $data} }
            # Compute the signal merge for this card. State for the WC
            # board (incl. gameOver so the WC can render the badge),
            # plus chrome (overlay timestamp).
            let state = $new_meta.state
            let tiles = ($state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
            let lmid = $new_meta | get last_move_id? | default $changed_id
            let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
            let signal_patch = ({
              games: {$changed_id: {tiles: $tiles, gameOver: ($state | get game_over? | default false)}}
              meta:  {$changed_id: {playedMs: $played_ms}}
            } | to datastar-patch-signals)
            # Structural change only fires the morph: a brand-new card
            # has to appear in the DOM, signals alone can't add an
            # element. Existing-game snapshot updates skip the morph
            # entirely -- chrome + board both flow through signals,
            # so the WC's animation isn't disturbed.
            if $is_new_card {
              let html_patch = (render-games-list-from-data $req $new_data
                                | to datastar-patch-elements --selector ".games-list" --id (random uuid))
              {out: [$signal_patch, $html_patch], next: $new_data}
            } else {
              {out: [$signal_patch], next: $new_data}
            }
          } $initial_data
        | flatten
        | to sse
      }
    })

    (route {method: GET path-matches: "/sse-wc/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # The actor owns game state; the SSE handler is a thin reader of
      # this game's snapshot stream. --from $game_id includes the
      # games_topic frame at that id and everything after; the
      # threshold-gate buffers it down to just the latest snapshot for
      # the initial render.
      .cat --follow -T $"game.($game_id).*" --from $game_id
      | frames-to-states
      | threshold-gate-states
      | states-to-wc-signals
      | html-to-patches
      | to sse
    })

    (route {method: GET path: "/new"} {|req ctx|
      # Mint a games_topic frame for this user and 302 to /play/<id>.
      # Session is auto-claimed from any legacy `player` cookie or
      # minted fresh. The user_id stays stable across `/new` calls.
      let existing = (resolve-session $req)
      let session = if $existing == null { mint-session (random uuid) } else { $existing }
      let games_topic = $"player.($session.user_id).games"
      let new_frame = (null | .append $games_topic)
      let location = ($req | href $"/play/($new_frame.id)")
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $location}}} }
      | session-cookies set $session
    })

    (route {method: GET path: "/"} {|req ctx|
      # Splash: marketing landing. PLAY NOW is the only thing the
      # visitor needs to see. Cookie is minted on /new, not here --
      # nothing to attribute to a player on the splash.
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      # Splash board: replay oleksii_lisovyi's 4096 / score-61640 run.
      # Pick a random starting frame so each viewer enters at a
      # different moment in the game. SSE streams subsequent states.
      # Fallback to the final-state snapshot if the store has no frames
      # (dev server / preview environments).
      let all_states = $SPLASH_STATES
      let fallback_state = {
        tiles: [
          {id: 2881 r: 0 c: 0 value: 2}    {id: 2873 r: 1 c: 0 value: 4}
          {id: 2864 r: 2 c: 0 value: 8}    {id: 2882 r: 3 c: 0 value: 2}
          {id: 2879 r: 0 c: 1 value: 4}    {id: 2874 r: 1 c: 1 value: 8}
          {id: 2861 r: 2 c: 1 value: 16}   {id: 2850 r: 3 c: 1 value: 32}
          {id: 2844 r: 0 c: 2 value: 8}    {id: 2749 r: 1 c: 2 value: 64}
          {id: 2678 r: 2 c: 2 value: 256}  {id: 2779 r: 3 c: 2 value: 64}
          {id: 581  r: 0 c: 3 value: 4096} {id: 1866 r: 1 c: 3 value: 1024}
          {id: 2327 r: 2 c: 3 value: 512}  {id: 2564 r: 3 c: 3 value: 256}
        ]
        ghosts: [] next_id: 2883 score: 61640 game_over: true
      }
      let start_pos = if ($all_states | is-empty) { 0 } else {
        random int 0..(($all_states | length) - 1)
      }
      let initial_state = if ($all_states | is-empty) {
        $fallback_state
      } else {
        $all_states | get $start_pos
      }
      ([
        # Splash hero. Two stacked flex rows:
        #   1) the title on its own row, full width;
        #   2) a 2-column row (.lede + .preview) that wraps to single
        #      column on narrow viewports.
        # Each column is itself a flex column. data-sse on the section
        # so SSE patches (targeting #splash-board + #splash-counter by
        # id) flow into descendants regardless of grouping.
        (SECTION {
          class: "hero"
          "data-sse": ""
          "data-init": ("@get('" + ($req | href "/sse/splash") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
          # Seed signals the splash board needs on first paint. SSE
          # patches overwrite both as bus.splash.seek frames arrive.
          # The wire shape strips per-tile animation hints to match
          # what the SSE handler emits.
          "data-signals": ({
            splashState: {
              tiles: ($initial_state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
              gameOver: ($initial_state | get game_over? | default false)
            }
            splashPos: $start_pos
          } | to json --raw)
        }
          (H2 "2048, in Nushell!")
          (DIV {class: "splits"}
            (DIV {class: "lede"}
              (P {class: "desc"} "The sliding-tile puzzle, served from a few hundred lines of shell script.")
              (kbd-btn "n" --prefix "Play " --suffix "ow" --variant primary --href ($req | href "/new") --style "margin-top: 1rem;")
              (UL {class: "callouts"}
                (LI (A {href: ($req | href "/notes/the-rules")} "never played?")
                    (SPAN {class: "callout-desc"} "the basic rules"))
                (LI (A {href: ($req | href "/notes/backstory")} "2048 is a broken game")
                    (SPAN {class: "callout-desc"} "how a clone of a clone ate Threes!"))
                (LI (A {href: ($req | href "/notes/in-nushell")} "in Nushell?")
                    (SPAN {class: "callout-desc"} "how this is built"))))
            (DIV {class: "preview"}
              (DIV {class: "credit"}
                (P
                  "replay of " (A {href: ($req | href $"/by/($SPLASH_PLAYER_ID)")} "oleksii_lisovyi") "'s "
                  (if ($SPLASH_DATE | is-empty) { "" } else { $"($SPLASH_DATE) " })
                  "run")
                (P "4096 in the top, right corner, score 61,640, on move 1874")
                (P "best on the site to date"))
              (render-tag "game-board" {id: "splash-board" "data-attr:state": "JSON.stringify($splashState)"})
              (DIV {class: "splash-progress"}
                (INPUT {
                  id: "splash-slider"
                  class: "splash-slider"
                  type: "range"
                  min: "0"
                  max: (($SPLASH_STATES | length | default 1) - 1 | into string)
                  # `n` carries the state count so the interval can wrap.
                  # The auto-tick advances $pos and posts; the input
                  # handler posts the user's drag. The bus does the rest.
                  "data-signals": $'{"pos": ($start_pos), "n": (($SPLASH_STATES | length | default 1))}'
                  "data-bind:pos": ""
                  "data-on:input__debounce.120ms": ("@post('" + ($req | href "/splash/seek") + "')")
                  "data-on-interval__duration.1200ms": ("$pos = ($pos + 1) % $n; @post('" + ($req | href "/splash/seek") + "')")
                })
                (SPAN {
                  id: "splash-counter"
                  class: "splash-counter"
                  # Counter follows the SSE-emitted $splashPos so it
                  # stays paired with the board state on the wire,
                  # regardless of what each viewer's local slider $pos
                  # ticked to.
                  "data-text": $"'move: ' + $splashPos + ' of ' + (($SPLASH_STATES | length | default 1) - 1)"
                } ""))
              # Audio toggle renders as <a href="#"> (kbd-btn does this when
              # --href is set); JS preventDefaults the click. Avoids webkit's
              # VT button-opacity bug -- see CLAUDE.md.
              (P {class: "splash-audio-credit"}
                (kbd-btn "p"
                  --prefix "(()) "
                  --suffix "lay"
                  --class "audio-toggle"
                  --href "#"
                  --aria-label "play audio")
                (SPAN " Out Stands -- ")
                (A {href: ($req | href "/mobygratis-license.txt") target: "_blank" rel: "noopener"} "mobygratis"))
              (AUDIO {id: "splash-audio" src: ($req | href "/mobygratis-out-stands.mp3") preload: "auto" loop: ""} ""))))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, encapsulated board web component."
            --body-class "splash"
            --sse true)
    })

    (route {method: GET path: "/my/games"} {|req ctx|
      # Your library. Session-required: no session = nothing to show
      # (visitors get a "start a game" prompt rather than someone
      # else's data). A legacy `player` cookie is one-shot claimed
      # into a session here.
      let session = (resolve-session $req)
      let games = if $session == null { [] } else {
        try { .cat -T $"player.($session.user_id).games" | reverse } catch { [] }
      }
      # Two nested signals keyed by game id. Each card binds via
      # data-attr to $games[<id>] (WC board state) and $meta[<id>]
      # (overlay timestamp + status badge). Live SSE patches merge
      # per-game updates into both signals; no HTML re-render needed
      # for snapshot changes.
      let games_signal = $games | reduce -f {} {|f acc|
        let resumed = (resume-game $f.id)
        let tiles = ($resumed.state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
        $acc | upsert $f.id {tiles: $tiles, gameOver: ($resumed.state | get game_over? | default false)}
      }
      let meta_signal = $games | reduce -f {} {|f acc|
        let resumed = (resume-game $f.id)
        let lmid = $resumed | get follow_from_id? | default $f.id
        let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
        $acc | upsert $f.id {playedMs: $played_ms}
      }
      let body = ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
              (kbd-btn "esc" --suffix " home" --href ($req | href "/"))
              (SPAN {class: "sep"} "·")
              (A {href: ($req | href "/my/games")} "my games")
            ]
            --right [
              (kbd-btn "n" --suffix "ew game" --href ($req | href "/new"))
            ])
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
          (P {class: "hint empty-state"} (if $session == null { "no session yet -- start a game to get one." } else { "no games yet." })))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "my games -- nu2048"
            --body-class "games-view"
            --sse ($session != null)
            --body-attrs (if $session == null { {} } else {
              {
                "data-sse": ""
                "data-init": ("@get('" + ($req | href "/sse/games") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
                "data-signals": ({games: $games_signal, meta: $meta_signal} | to json --raw)
              }
            }))
      if $session == null { $body } else { $body | session-cookies set $session }
    })

    (route {method: GET path-matches: "/watch/:game_id"} {|req ctx|
      # Public spectator view. No auth, no kbd controls -- just the
      # board + score + state badge. Renders the <game-board> WC and
      # subscribes to /sse-wc/<game_id> which patches $boardState,
      # $score, and $gameStatus signals. The WC observes its `state`
      # attribute (mirrored from $boardState via data-attr:state) and
      # owns the 3-phase slide/merge/spawn animation internally.
      let game_id = $ctx.game_id
      let owner_frame = try { .get $game_id } catch { null }
      if $owner_frame == null {
        "Not Found" | metadata set { merge {'http.response': {status: 404}} }
      } else {
        let owner_id = $owner_frame.topic | str replace "player." "" | str replace ".games" ""
        let owner_short = $owner_id | str substring 0..7
        let game_id_short = $game_id | str substring 0..7
        let home_href = ($req | href "/")
        ([
          (DIV {class: "page"}
            (breadcrumb
              --left [
                (kbd-btn "esc" --suffix " home" --href $home_href)
                (SPAN {class: "sep"} "·")
                (A {href: ($req | href $"/by/($owner_id)")} $"by ($owner_short)")
                (SPAN {class: "sep"} "·")
                (A {class: "game-id" href: ($req | href $"/watch/($game_id)")} $game_id_short)
              ]
              --right [
                (kbd-btn "n" --suffix "ew game" --href ($req | href "/new"))
              ])
            (DIV {class: "play-layout"}
              # Same grid skeleton as /play, but only the score row +
              # board column. Help cell stays empty.
              (DIV {class: "board-controls"} (render-score 0))
              (DIV {
                class: "column"
                "data-sse": ""
                "data-init": ("@get('" + ($req | href $"/sse-wc/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
                # Seed the WC + chrome signals so first paint is sane
                # before SSE lands. The state-badge ("you win!" /
                # "game over") is owned by the WC's shadow DOM.
                "data-signals": "{boardState: {tiles: [], gameOver: false}, score: 0, gameStatus: ''}"
              }
                (DIV {id: "board-wrap"}
                  (render-tag "game-board" {"data-attr:state": "JSON.stringify($boardState)"})))))
        ] | layout $req $REV $DATASTAR_JS_PATH
              --title $"watching ($game_id_short) -- nu2048"
              --body-class "watch"
              --sse true)
      }
    })

    (route {method: GET path-matches: "/by/:player_id"} {|req ctx|
      # Public per-player games view. Same shape as /my/games but
      # takes the id from the URL (no cookie required). Used for
      # crediting featured games on the splash. Currently static
      # (no /sse/by/<id> handler yet), so $games is seeded once and
      # never patched -- each card's board renders to its snapshot
      # state and stays put.
      let player_id = $ctx.player_id
      let pid_short = $player_id | str substring 0..7
      let games_topic = $"player.($player_id).games"
      let games = try { .cat -T $games_topic | reverse } catch { [] }
      let games_signal = $games | reduce -f {} {|f acc|
        let resumed = (resume-game $f.id)
        let tiles = ($resumed.state.tiles | each {|t| {id: $t.id, r: $t.r, c: $t.c, value: $t.value} })
        $acc | upsert $f.id {tiles: $tiles, gameOver: ($resumed.state | get game_over? | default false)}
      }
      let meta_signal = $games | reduce -f {} {|f acc|
        let resumed = (resume-game $f.id)
        let lmid = $resumed | get follow_from_id? | default $f.id
        let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
        $acc | upsert $f.id {playedMs: $played_ms}
      }
      ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
              (kbd-btn "esc" --suffix " home" --href ($req | href "/"))
              (SPAN {class: "sep"} "·")
              (A {href: ($req | href $"/by/($player_id)")} $"games by ($pid_short)")
            ]
            --right [
              (kbd-btn "n" --suffix "ew game" --href ($req | href "/new"))
            ])
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f --href ($req | href $"/watch/($f.id)") }))
          (P {class: "hint empty-state"} "no games yet."))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title $"games by ($pid_short) -- nu2048"
            --body-class "games-view"
            --body-attrs {"data-signals": ({games: $games_signal, meta: $meta_signal} | to json --raw)})
    })

    (route {method: GET path-matches: "/play/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # Owner-or-404. Anonymous visitors and visitors whose session
      # doesn't own this game get a not-found -- /watch/<game_id> is
      # the public read-only path.
      let session = (resolve-session $req)
      # Owner = the player on whose `player.<id>.games` topic this
      # game's creating frame was appended. Read directly so we don't
      # race the snapshot-actor.
      let owner_frame = try { .get $game_id } catch { null }
      let owner_id = if $owner_frame == null { "" } else {
        $owner_frame.topic | str replace "player." "" | str replace ".games" ""
      }
      if $session == null or ($session.user_id != $owner_id) {
        "Not Found" | metadata set { merge {'http.response': {status: 404}} }
      } else {
        let player_id = $session.user_id
      let home_href = ($req | href "/")
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      let game_id_short = $game_id | str substring 0..7
      ([
        (DIV {class: "page"}
          # $lastReqId signal fires for every move (echo or snapshot)
          # from the SSE pipeline. window.onAck (script.js) no-ops
          # unless reqId matches the local pending probe; this is what
          # clears the pending edge and records RTT now that #game has
          # no data-rev attribute. data-on-signal-patch fires only on
          # signal patches (not on mount), so it's safe even though the
          # deferred script.js defines window.onAck after Datastar
          # initializes.
          (DIV {"data-on-signal-patch": "window.onAck($lastReqId)" hidden: ""})
          # Breadcrumb header: left = path with shortcuts adjacent to
          # their link targets ([esc] sits next to "past games" because
          # esc is its keyboard shortcut). Right = top-level actions.
          # The game-id is a self-link so it can be right-clicked to
          # copy a bookmarkable URL.
          (breadcrumb
            --left [
              (kbd-btn "esc" --suffix " home" --href $home_href)
              (SPAN {class: "sep"} "·")
              (A {class: "game-id" href: ($req | href $"/play/($game_id)")} $game_id_short)
            ]
            --right [
              # Same game, spectator view -- right-click to share.
              (A {href: ($req | href $"/watch/($game_id)")} "watch")
              (kbd-btn "n" --suffix "ew game" --href ($req | href "/new"))
            ])
          (DIV {class: "play-layout"}
            # Grid template areas (CSS): score row tops the board column,
            # help spans rows 1-2 so its top aligns with the BOARD top,
            # not the score row above it.
            (DIV {class: "board-controls"} (render-score 0))
            (DIV {
              class: "column"
              "data-sse": ""
              "data-init": ("@get('" + ($req | href $"/sse-wc/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
            }
              # #board-wrap stays as the positioning anchor for the
              # state-badge overlay and as the target for the data-pending
              # edge-line indicator script.js sets on keydown. The board
              # itself is the WC, observing $boardState via Datastar's
              # data-attr:state mirroring.
              # The state-badge ("you win!" / "game over") lives inside
              # the WC's shadow DOM, derived from boardState.gameOver
              # and the tile values. Same look + behavior on every
              # surface that renders <game-board>.
              (DIV {id: "board-wrap"}
                (render-tag "game-board" {"data-attr:state": "JSON.stringify($boardState)"})))
            # Help panel: each key is a real button that triggers the
            # move via the existing [data-intent] click delegate in
            # script.js.
            (ASIDE {class: "help"}
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "left")
                (kbd-btn "h" --intent "h") (kbd-btn "←" --intent "h"))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "down")
                (kbd-btn "j" --intent "j") (kbd-btn "↓" --intent "j"))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "up")
                (kbd-btn "k" --intent "k") (kbd-btn "↑" --intent "k"))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "right")
                (kbd-btn "l" --intent "l") (kbd-btn "→" --intent "l"))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "undo")
                (kbd-btn "u" --intent "undo")
                (SPAN {}))))
        )
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, encapsulated board web component."
            --body-class "play"
            --sse true
            --body-attrs {
              "data-player-id": $player_id
              "data-game-id": $game_id
              "data-move-url": ($req | href "/move")
              "data-signals": $"{playerId: '($player_id)', gameId: '($game_id)', score: 0, lastReqId: '', gameStatus: '', boardState: {tiles: [], gameOver: false}}"
            }
        | session-cookies set $session)
      }
    })

    (route {method: GET} {|req ctx| .static $STATIC_DIR $req.path})
  ]
}
