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
def render-game-card [req: record game_frame: record]: nothing -> record {
  let resumed = (resume-game $game_frame.id)
  render-card-from-state $req $game_frame.id $resumed.state $resumed.moves $resumed.follow_from_id
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
      # page knows which game it's on). Body shape: {playerId, gameId,
      # intent, reqId}.
      let signals = $in | from datastar-signals $req
      let game_id = $signals | get gameId? | default ""
      let intent = $signals | get intent? | default ""
      let req_id = $signals | get reqId? | default ""
      if ($game_id | is-empty) {
        null | metadata set { merge {'http.response': {status: 400}} }
      } else {
        let topic = $"game.($game_id).move"
        if $intent == "undo" {
          null | .append $topic --meta {kind: "undo" req_id: $req_id}
        } else if $intent == "" {
          # RTT-measurement ping: client sends an empty intent so the SSE
          # echoes a no-op state. Don't persist -- ephemeral keeps the
          # move log clean.
          null | .append $topic --ttl ephemeral --meta {intent: $intent req_id: $req_id}
        } else {
          # Covers h/j/k/l.
          null | .append $topic --meta {intent: $intent req_id: $req_id}
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      }
    })

    (route {method: GET path: "/sse/splash"} {|req ctx|
      # Replay the splash game's snapshot stream on loop. Each
      # connection picks a random start frame; ~600ms per step so the
      # board feels deliberate. Each patch rides a view-transition so
      # tiles slide between states (paired by vt-name on tile.id).
      #
      # Multiplexes two patch streams per tick:
      #   1. <#splash-board> -- the board state
      #   2. <#splash-slider> -- the progress slider value
      let states = $SPLASH_STATES
      if ($states | is-empty) {
        null | metadata set { merge {'http.response': {status: 204}} }
      } else {
        let n = $states | length
        let start = random int 0..($n - 1)
        # Stream forever; `to sse` consumes lazily.
        1..
        | each {|i|
            sleep 600ms
            let idx = (($start + $i) mod $n)
            let state = $states | get $idx
            let board_patch = (
              (DIV {id: "splash-board"} ($state | render-board "splash"))
              | to datastar-patch-elements --use-view-transition --id (random uuid)
            )
            let slider_patch = (
              (INPUT {id: "splash-slider" type: "range" min: "0" max: ($n - 1 | into string) value: ($idx | into string) "data-preserve-attr": "value"})
              | to datastar-patch-elements --id (random uuid)
            )
            let counter_patch = (
              (SPAN {id: "splash-counter" class: "splash-counter"} $"($idx) of ($n - 1)")
              | to datastar-patch-elements --id (random uuid)
            )
            [$board_patch $slider_patch $counter_patch]
          }
        | flatten
        | to sse
      }
    })

    (route {method: POST path: "/splash/seek"} {|req ctx|
      # Scrub the splash slider. Datastar sends `{pos: <int>}`; we
      # pub-broadcast it onto the bus topic so any active SSE loops
      # could pick it up and jump. (Stub for now: ephemeral frame, no
      # consumer wired in. Mechanism is what matters; loops can opt in
      # to bus-sync later.)
      let signals = $in | from datastar-signals $req
      let pos = $signals | get pos? | default 0 | into int
      null | .append "bus.splash.seek" --ttl ephemeral --meta {pos: $pos}
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
        .cat --follow --pulse 450
        | pulse-keepalive
        | generate {|item data|
            if ('event' in $item) { return {out: $item, next: $data} }
            if ($head != null and $item.id <= $head) { return {next: $data} }
            let new_data = if $item.topic == $games_topic {
              let snap = try { .last $"game.($item.id).snapshot" } catch { null }
              if $snap == null { $data } else { $data | upsert $item.id $snap.meta }
            } else if (($item.topic | str ends-with ".snapshot") and (($item.meta? | get player_id? | default "") == $player_id)) {
              let game_id = $item.topic | str replace "game." "" | str replace ".snapshot" ""
              $data | upsert $game_id $item.meta
            } else { $data }
            if $new_data == $data { return {next: $data} }
            # No --use-view-transition: morphdom diffs in place, only the
            # changed card actually mutates. With VT, every card got swept
            # into the snapshot/cross-fade -- which (a) re-ran the mute
            # animation on every board even when its content was unchanged,
            # and (b) flashed the entire list un-dimmed for one
            # transition-length while the cross-fade played.
            let patch = (render-games-list-from-data $req $new_data
                          | to datastar-patch-elements --selector ".games-list" --id (random uuid))
            {out: $patch, next: $new_data}
          } $initial_data
        | to sse
      }
    })

    (route {method: GET path-matches: "/sse/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # The actor owns game state; the SSE handler is a thin reader of
      # this game's snapshot stream (plus ephemeral view-toggles).
      # --from $game_id includes the games_topic frame at that id and
      # everything after; the threshold-gate buffers it down to just the
      # latest snapshot for the initial render. `pulse-keepalive` runs
      # first so the rendering stages can stay pulse-agnostic.
      .cat --follow --pulse 450 -T $"game.($game_id).*" --from $game_id
      | pulse-keepalive
      | frames-to-states
      | threshold-gate-states
      | states-to-html
      | html-to-patches
      | to sse
    })

    (route {method: GET path: "/script.js"} {|req ctx|
      .static $STATIC_DIR "/script.js"
    })

    (route {method: GET path: "/styles.css"} {|req ctx|
      .static $STATIC_DIR "/styles.css"
    })

    (route {method: GET path: "/ellie.png"} {|req ctx|
      .static $STATIC_DIR "/ellie.png"
    })

    (route {method: GET path: "/og.png"} {|req ctx|
      .static $STATIC_DIR "/og.png"
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
      let initial_state = if ($all_states | is-empty) {
        $fallback_state
      } else {
        $all_states | get (random int 0..(($all_states | length) - 1))
      }
      ([
        (SECTION {class: "hero"}
          # Outer wrapper holds the SSE subscription (never morphed);
          # #splash-board + #splash-slider inside are the patch targets.
          (DIV {
            class: "preview"
            "data-sse": ""
            "data-init": ("@get('" + ($req | href "/sse/splash") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
          }
            (DIV {class: "splash-progress"}
              (INPUT {
                id: "splash-slider"
                class: "splash-slider"
                type: "range"
                min: "0"
                max: (($SPLASH_STATES | length | default 1) - 1 | into string)
                value: "0"
                "data-preserve-attr": "value"
                "data-on-input": ("@post('" + ($req | href "/splash/seek") + "', {pos: parseInt(event.target.value, 10)})")
              })
              (SPAN {id: "splash-counter" class: "splash-counter"} (if ($SPLASH_STATES | is-empty) { "0 of 0" } else { $"0 of (($SPLASH_STATES | length) - 1)" })))
            (DIV {id: "splash-board"} ($initial_state | render-board "splash"))
            (P {class: "splash-credit"}
              "replay of " (A {href: ($req | href $"/by/($SPLASH_PLAYER_ID)")} "oleksii_lisovyi") "'s "
              (if ($SPLASH_DATE | is-empty) { "" } else { $"($SPLASH_DATE) " })
              "run -- 4096 in the corner, score 61,640 (best on the site to date)"))
          (DIV {class: "lede"}
            (H2 "2048, in Nushell!")
            (P "The sliding-tile puzzle, served from a few hundred lines of shell script.")
            (A {class: "play-now" href: ($req | href "/new")} "play now")
            (UL {class: "callouts"}
              (LI (A {href: ($req | href "/notes/the-rules")} "never played?")
                  (SPAN {class: "callout-desc"} "the basic rules"))
              (LI (A {href: ($req | href "/notes/backstory")} "what the heck is 2048?")
                  (SPAN {class: "callout-desc"} "where it came from"))
              (LI (A {href: ($req | href "/notes/in-nushell")} "in Nushell?")
                  (SPAN {class: "callout-desc"} "how this is built")))))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, view-transition tile slides."
            --body-class "splash")
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
      let body = ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
              (A {href: ($req | href "/")} "home")
              (kbd-btn "esc" --href ($req | href "/"))
              (SPAN {class: "sep"} "·")
              (A {href: ($req | href "/my/games")} "my games")
            ]
            --right [
              (A {href: ($req | href "/new")} "new game")
              (kbd-btn "n" --href ($req | href "/new"))
            ])
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
          (P {class: "hint empty-state"} (if $session == null { "no session yet -- start a game to get one." } else { "no games yet." })))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "my games -- nu2048"
            --body-class "games-view"
            --body-attrs (if $session == null { {} } else {
              {
                "data-sse": ""
                "data-init": ("@get('" + ($req | href "/sse/games") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
              }
            }))
      if $session == null { $body } else { $body | session-cookies set $session }
    })

    (route {method: GET path-matches: "/by/:player_id"} {|req ctx|
      # Public per-player games view. Same shape as /my/games but
      # takes the id from the URL (no cookie required). Used for
      # crediting featured games on the splash.
      let player_id = $ctx.player_id
      let pid_short = $player_id | str substring 0..7
      let games_topic = $"player.($player_id).games"
      let games = try { .cat -T $games_topic | reverse } catch { [] }
      ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
              (A {href: ($req | href "/")} "home")
              (kbd-btn "esc" --href ($req | href "/"))
              (SPAN {class: "sep"} "·")
              (A {href: ($req | href $"/by/($player_id)")} $"games by ($pid_short)")
            ]
            --right [
              (A {href: ($req | href "/new")} "new game")
              (kbd-btn "n" --href ($req | href "/new"))
            ])
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
          (P {class: "hint empty-state"} "no games yet."))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title $"games by ($pid_short) -- nu2048"
            --body-class "games-view")
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
      # Render an EMPTY board as the placeholder: same dimensions (grid cells
      # fill it) so no layout jump, but no tiles in the DOM yet. When the SSE
      # init patch arrives with the real tiles, they're unpaired (:only-child)
      # which fires the spawn pop-in -- otherwise paired tiles would just
      # cross-fade with no animation.
      let home_href = ($req | href "/")
      let placeholder = {tiles: [] next_id: 1 score: 0 game_over: false} | render-game
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      let game_id_short = $game_id | str substring 0..7
      ([
        (DIV {class: "page"}
          # Breadcrumb header: left = path with shortcuts adjacent to
          # their link targets ([esc] sits next to "past games" because
          # esc is its keyboard shortcut). Right = top-level actions.
          # The game-id is a self-link so it can be right-clicked to
          # copy a bookmarkable URL.
          (breadcrumb
            --left [
              (A {href: $home_href} "home")
              (kbd-btn "esc" --href $home_href)
              (SPAN {class: "sep"} "·")
              (A {class: "game-id" href: ($req | href $"/play/($game_id)")} $game_id_short)
            ]
            --right [
              (A {href: ($req | href "/new")} "new game")
              (kbd-btn "n" --href ($req | href "/new"))
            ])
          (DIV {class: "play-layout"}
            # Grid template areas (CSS): score row tops the board column,
            # help spans rows 1-2 so its top aligns with the BOARD top,
            # not the score row above it.
            (DIV {class: "board-controls"} (render-score 0))
            # data-init lives on .column (never patched) so the SSE fetch +
            # connection signal survive morphs of #game.
            (DIV {
              class: "column"
              "data-sse": ""
              "data-init": ("@get('" + ($req | href $"/sse/($game_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
            }
              $placeholder)
            # Help panel: each key is a real button that triggers the move
            # via the existing [data-intent] click delegate in script.js.
            # The fx-tuner toggle lives here too instead of as a separate
            # floating tab.
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
                (SPAN {}))
              (DIV {class: "help-row help-fx"}
                (SPAN {class: "label"} "tuner")
                (kbd-btn "fx" --class "fx-toggle" --bracketless)
                (SPAN {}))
              (render-tuner)))
        )
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, view-transition tile slides."
            --body-class "play"
            --body-attrs {
              "data-player-id": $player_id
              "data-game-id": $game_id
              "data-move-url": ($req | href "/move")
              "data-signals": $"{playerId: '($player_id)', gameId: '($game_id)'}"
            }
        | session-cookies set $session)
      }
    })
  ]
}
