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
  try { .cat -T $"game.snapshot.($SPLASH_GAME_ID)" | get meta.state } catch { [] }
}
# YYYY-MM-DD of the run (SCRU128 timestamp 1779014675.541 = 2026-05-17).
# Hardcoded for now -- `.id unpack` isn't in scope at module-init time;
# upstream fix pending. Re-derive when the splash game changes.
let SPLASH_DATE = "2026-05-17"
# Player id whose game we are replaying. Used to deep-link the credit
# to /by/<id> so visitors can see oleksii_lisovyi's other games.
const SPLASH_PLAYER_ID = "542221d8-be77-4fac-91cb-1bfa49ae3b2a"

# Register the xs snapshot-actor (singleton): it watches every
# `player.*.games` + `game.move.*` frame and writes the canonical
# `game.snapshot.<id>` (ttl: last:1). Requires `--store` + `--services`;
# guarded so that test.nu, which sources serve.nu without a store, stays
# happy. Re-registering on each startup replaces the running actor (per
# xs's `xs.actor.<name>.create` semantics), so this is restart-safe.
if ($HTTP_NU.store? | default null) != null and ($HTTP_NU.services? | default false) {
  # `--ttl last:1` caps each registration topic to a single frame:
  # --watch reloads of serve.nu still re-append, but xs evicts the
  # previous frame as the new one lands so the topic never grows.
  # The active actor still receives the new create live and
  # self-terminates; the spawn that replaces it uses the surviving
  # frame. Same shape for `game.nu` -- it's a module topic
  # (`xs.module.game`) the snapshot-actor consumes via `use game *`.
  open ($SCRIPT_DIR | path join "tfe" "game.nu")               | .append xs.module.game                    --ttl last:1
  open ($SCRIPT_DIR | path join "tfe" "snapshot-actor.nu")     | .append xs.actor.snapshot-actor.create    --ttl last:1
  open ($SCRIPT_DIR | path join "tfe" "leaderboard-actor.nu")  | .append xs.actor.leaderboard-actor.create --ttl last:1
  open ($SCRIPT_DIR | path join "tfe" "presence-actor.nu")     | .append xs.actor.presence-actor.create    --ttl last:1
}

# Render a card from a games_topic frame (the initial page render). Reads
# the game's head snapshot for state, then defers to render-card-from-state.
# Caller chooses the destination via `--href`; defaults to /play.
def render-game-card [req: record game_frame: record --href: string]: nothing -> record {
  let resumed = game-head $game_frame.id
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
      let session = resolve-session $req
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
        let topic = $"game.move.($game_id)"
        let meta_base = {
          user_id: $session.user_id
          session_id: $session.session_id
          req_id: $req_id
        }
        if $intent == "undo" {
          null | .append $topic --meta ($meta_base | upsert kind "undo")
        } else {
          # Covers h/j/k/l. Empty-intent RTT pings are gone -- liveness
          # is owned by /presence/ping now.
          null | .append $topic --meta ($meta_base | upsert intent $intent)
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      }
    })

    (route {method: GET path-matches: "/sse/splash/:tabId"} {|req ctx|
      # Per-tab reader: subscribe to bus.splash.seek.<tabId> and emit a
      # board + pos-signal patch per frame. The cadence lives in the
      # client (the slider's data-on:interval auto-tick and the
      # scrub-end post), so each tab drives its own queue and only sees
      # its own seeks. Without per-tab scoping, N open tabs all post
      # into a shared topic at 1.2s each, racing $pos to chaos.
      #
      # `--last 1` flushes the current pos to a freshly-connected
      # viewer right away (no gap before they see a board).
      let states = $SPLASH_STATES
      if ($states | is-empty) {
        # Empty store: no board to drive, but presence still applies.
        presence-stream | to sse
      } else {
        let n = $states | length
        let topic = $"bus.splash.seek.($ctx.tabId)"
        # No `let board_stream = ...` -- Nushell `let` would COLLECT
        # the infinite `.cat --follow` before binding, hanging the
        # handler. Pipe the stream straight into `interleave`.
        .cat --last 1 --follow -T $topic
        | where ($it.topic? | default "") == $topic
        | each {|f|
            let pos = (($f.meta? | default {} | get pos? | default 0) | into int) mod $n
            let state = $states | get $pos
            # WC variant: ship the state as a signal; <game-board>
            # picks it up via data-attr:state and runs its own
            # animation. The counter is signal-bound on the client
            # side (data-text on $pos), so we just need to push the
            # pos number here. Strip per-tile animation hints from the
            # wire payload; the WC diffs by id.
            let board = $state | state-for-wc
            {splashState: $board, splashPos: $pos}
            | to datastar-patch-signals
          }
        | interleave { presence-stream }
        | to sse
      }
    })

    (route {method: POST path: "/presence/ping"} {|req ctx|
      # Site-wide health/presence heartbeat. Replaces the per-/play
      # empty-intent /move probe. Body: {tabId, scope, gameId?}. Each
      # ping appends an ephemeral frame -- not stored, only live
      # subscribers (the presence-actor) observe it. 204 ack lets the
      # client flip body[data-conn]=ok; fetch errors / non-204 trip
      # data-conn=down.
      let body = try { $in | from json } catch { {} }
      let tab_id = $body | get tabId? | default ""
      let scope  = $body | get scope?  | default ""
      let game_id = $body | get gameId? | default ""
      let session = resolve-session $req
      let user_id = if $session == null { "" } else { $session.user_id }
      if ($tab_id | is-empty) {
        "missing tabId" | metadata set { merge {'http.response': {status: 400}} }
      } else {
        null | .append "_presence.ping" --ttl ephemeral --meta {
          tabId: $tab_id
          scope: $scope
          gameId: $game_id
          user_id: $user_id
        }
        null | metadata set { merge {'http.response': {status: 204}} }
      }
    })

    (route {method: POST path: "/splash/seek"} {|req ctx|
      # The splash cadence -- both auto-tick (data-on-interval) and
      # the scrub-end commit -- posts here. Datastar serializes all
      # signals into the body; we pluck pos + tabId. Each tab's posts
      # land on its own bus topic so a single tab's autoplay can't
      # disturb another tab's reader.
      let signals = $in | from datastar-signals $req
      let pos = $signals | get pos? | default 0 | into int
      let tab_id = $signals | get tabId? | default ""
      if ($tab_id | is-empty) {
        "missing tabId" | metadata set { merge {'http.response': {status: 400}} }
      } else {
        null | .append $"bus.splash.seek.($tab_id)" --ttl last:1 --meta {pos: $pos}
        null | metadata set { merge {'http.response': {status: 204}} }
      }
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
      let session = resolve-session $req
      if $session == null {
        null | metadata set { merge {'http.response': {status: 401}} }
      } else {
        let player_id = $session.user_id
        let games_topic = $"player.($player_id).games"
        # Seed $data with the player's existing games + their latest
        # snapshots so the first push is consistent with the initial
        # page render. While scanning, track the max frame id observed
        # -- this is our live cursor: everything <= it is already in
        # initial_data; everything > it is genuinely new.
        let game_frames = try { .cat -T $games_topic } catch { [] }
        let scan = $game_frames | each {|f|
          let snap = .last $"game.snapshot.($f.id)"
          let max_id = if $snap == null { $f.id } else { [$f.id, $snap.id] | sort | last }
          {game_id: $f.id, snap: $snap, max_id: $max_id}
        }
        let initial_data = $scan | reduce -f {} {|r acc|
          if $r.snap == null { $acc } else { $acc | upsert $r.game_id $r.snap.meta }
        }
        # `--from <cursor>` resumes from the highest id we saw in the
        # scan. Empty-games case: `.id` mints a fresh scru128 (xs's
        # "now"), so the follow attaches to genuinely-future frames.
        # Previous shape used `.cat | last | get id` here, which
        # scanned the whole store (~28k frames) just to get the head
        # id and blocked SSE startup for seconds. No `let games_stream
        # =` binding -- nushell's let collects streaming pipelines.
        let cursor = if ($scan | is-empty) { (.id) } else { $scan | get max_id | sort | last }
        .cat --follow --from $cursor
        | generate {|item data|
            if ('event' in $item) { return {out: $item, next: $data} }
            let changed_id = if $item.topic == $games_topic {
              $item.id
            } else if (($item.topic | str starts-with "game.snapshot.") and (($item.meta? | get player_id? | default "") == $player_id)) {
              $item.topic | str substring 14..
            } else { null }
            if $changed_id == null { return {next: $data} }
            let new_meta = if $item.topic == $games_topic {
              # `default {}` first: a brand-new game may have no snapshot
              # yet, and `.last` on an empty topic yields an empty pipeline
              # that crashes `get` ("Pipeline empty") -- the `| default null`
              # alone never runs. See CLAUDE.md (Nushell Style).
              .last $"game.snapshot.($item.id)" | default {} | get meta? | default null
            } else { $item.meta }
            if $new_meta == null { return {next: $data} }
            let is_new_card = $changed_id not-in ($data | columns)
            let new_data = $data | upsert $changed_id $new_meta
            if $new_data == $data { return {next: $data} }
            # Compute the signal merge for this card. State for the WC
            # board with playedMs folded in for the overlay -- one
            # signal per game, the WC reads everything.
            let state = $new_meta.state
            let lmid = $new_meta | get last_move_id? | default $changed_id
            let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
            let wc_state = $state | state-for-wc | upsert playedMs $played_ms
            let signal_patch = ({
              games: {$changed_id: $wc_state}
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
        | interleave { presence-stream }
        | to sse
      }
    })

    (route {method: GET path: "/sse/presence"} {|req ctx|
      # Presence-only SSE for pages that have no per-page event
      # stream of their own (/leaderboard, /by/<id>). Identical wire
      # shape to the presence-stream interleaved into the other SSEs,
      # so client-side display code stays oblivious to which surface
      # delivered the patch.
      presence-stream | to sse
    })

    (route {method: GET path-matches: "/sse-wc/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # The actor owns game state; the SSE handler is a thin reader of
      # this game's snapshot stream. Snapshot-only (exact-match -T,
      # indexed): every move ack now flows through a snapshot --
      # state-changing as a durable one, no-op as an ephemeral one
      # carrying just the req_id -- so the SSE has no reason to follow
      # `game.move.<id>`. --from $game_id includes the games_topic
      # frame at that id and everything after; threshold-gate buffers
      # it down to just the latest snapshot for the initial render.
      # Interleaved with the site-wide presence stream so a /watch or
      # /play tab sees live "N people on this game" counts on the same
      # connection.
      # No `let board_stream = ...` -- Nushell `let` would COLLECT
      # the infinite `.cat --follow` before binding, hanging the
      # handler. See examples/2048/CLAUDE.md.
      .cat --follow -T $"game.snapshot.($game_id)" --from $game_id
      | frames-to-states
      | threshold-gate-states
      | states-to-wc-signals
      | html-to-patches
      | interleave { presence-stream }
      | to sse
    })

    (route {method: GET path: "/new"} {|req ctx|
      # Mint a games_topic frame for this user and 302 to /play/<id>.
      # Session is auto-claimed from any legacy `player` cookie or
      # minted fresh. The user_id stays stable across `/new` calls.
      let existing = resolve-session $req
      let session = if $existing == null { mint-session (random uuid) } else { $existing }
      let games_topic = $"player.($session.user_id).games"
      let new_frame = null | .append $games_topic
      let location = $req | href $"/play/($new_frame.id)"
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
      # Wrap modulus for the splash autoplay -- clamp to >=1 so the
      # empty-store case (dev/preview) doesn't drive `% 0` into NaN.
      let splash_n = [($all_states | length) 1] | math max
      # Per-tab id so this page's splash autoplay + seeks don't leak
      # into other open tabs. Threaded through both the SSE URL path
      # and the @post body (Datastar auto-serializes all signals).
      let tab_id = random uuid
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
          "data-init": ("@get('" + ($req | href $"/sse/splash/($tab_id)") + "', {retry: 'always', retryInterval: 100, retryScaler: 1, retryMaxCount: Infinity})")
          # Seed signals the splash board needs on first paint. SSE
          # patches overwrite splashState/splashPos as the per-tab
          # bus.splash.seek.<tabId> frames arrive. tabId itself rides
          # along on every @post so the seek handler knows where to
          # publish. The wire shape strips per-tile animation hints to
          # match what the SSE handler emits.
          # presence is seeded by layout.html (data-signals:presence__ifmissing).
          "data-signals": ({
            splashState: ($initial_state | state-for-wc)
            splashPos: $start_pos
            tabId: $tab_id
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
                (LI (A {href: ($req | href "/notes/why-is-this-so-addictive")} "why is this so addictive?!")
                    (SPAN {class: "callout-desc"} "the hooks, and the rabbit hole"))
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
                # data-attr:value pushes $pos into the WC; the WC emits
                # `scrub` on each integer-frame delta during pointer-lock
                # drag and `scrub-end` on release. The debounced scrub
                # handler updates the signal and posts. `n` is the wrap
                # modulus for the auto-tick -- clamped to >=1 (see
                # $splash_n above) so the empty-store dev/preview case
                # can't blow up the autoplay into NaN-land.
                (render-tag "scrub-knob" {
                  id: "splash-slider"
                  class: "splash-slider"
                  max: (($splash_n - 1) | into string)
                  "data-signals": $'{"pos": ($start_pos), "n": ($splash_n)}'
                  "data-attr:value": "$pos"
                  # `scrub` fires on every integer-frame step during drag --
                  # update $pos immediately so the counter (bound to $pos
                  # below) tracks the user's drag live, no debounce.
                  # `scrub-end` fires on pointer release; that's when we
                  # commit the seek to the server so the board catches up.
                  "data-on:scrub": "$pos = evt.detail.value"
                  "data-on:scrub-end": ("@post('" + ($req | href "/splash/seek") + "')")
                  "data-on-interval__duration.1200ms": ("$pos = ($pos + 1) % $n; @post('" + ($req | href "/splash/seek") + "')")
                })
                (SPAN {
                  id: "splash-counter"
                  class: "splash-counter"
                  # Counter follows the local $pos so it reflects the
                  # user's drag intent live. The board itself updates
                  # via $splashState on the SSE round-trip after the
                  # scrub-end POST, so it lags behind the counter while
                  # the drag is in flight -- counter is the user's
                  # cursor, board is the confirmed render.
                  "data-text": $"'move: ' + $pos + ' of ' + ($splash_n - 1)"
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

    (route {method: GET path: "/leaderboard"} {|req ctx|
      # Per-player top-5 by score. The leaderboard-actor maintains
      # `leaderboard.top` (ttl last:5) -- meta.entries is the canonical
      # table. `.last` is O(1); no scan-on-request. State for each
      # entry's board comes from `.last game.snapshot.<id>`, one cheap
      # head-lookup per row. Static page; v2 will SSE-follow
      # `leaderboard.top` for live re-renders.
      let head = .last leaderboard.top
      let entries = if $head == null { [] } else { $head.meta | get entries? | default [] }

      # Hydrate the per-row board signals from each entry's current
      # snapshot. Single signal per game: `playedMs` is folded into
      # state-for-wc's output so the WC reads everything via
      # data-attr:state.
      let hydrated = $entries | each {|e|
        let snap = .last $"game.snapshot.($e.game_id)"
        if $snap == null { null } else {
          let lmid = $snap.meta | get last_move_id? | default $e.game_id
          let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
          let state = $snap.meta.state | state-for-wc | upsert playedMs $played_ms
          {entry: $e, state: $state}
        }
      } | compact
      let games_signal = $hydrated | reduce -f {} {|h acc| $acc | upsert $h.entry.game_id $h.state }

      let rows = $hydrated | enumerate | each {|p|
        let rank = $p.index + 1
        let e = $p.item.entry
        let player_short = $e.player_id | str substring 0..7
        (LI {class: "leaderboard-row"}
          (SPAN {class: "rank"} $"#($rank)")
          (DIV {class: "row-card"}
            (render-card-from-state $req $e.game_id $p.item.state ($e | get moves? | default 0) "" --href ($req | href $"/watch/($e.game_id)")))
          (DIV {class: "row-meta"}
            (P {class: "score"} (($e.score | into string)))
            (P {class: "row-line"}
              "max tile " (SPAN {class: "max-tile"} ($e.max_tile | into string))
              " · "
              "moves " (SPAN {class: "moves"} ($e.moves | into string)))
            (P {class: "by"}
              "by " (A {href: ($req | href $"/by/($e.player_id)")} $player_short))))
      }

      let empty = $entries | is-empty
      ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
              (A {href: ($req | href "/leaderboard")} "leaderboard")
            ]
            --right [
              (kbd-btn "n" --suffix "ew game" --href ($req | href "/new"))
            ])
          (H1 {class: "leaderboard-title"} "leaderboard")
          (P {class: "leaderboard-lede"} "top 5 -- per-player best, in-flight or finished.")
          (if $empty {
            (P {class: "hint empty-state"} "no scored games tracked yet -- play one and check back.")
          } else {
            (UL {class: "leaderboard-list"} ...$rows)
          }))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "leaderboard -- nu2048"
            --body-class "leaderboard-view"
            --sse true
            --body-attrs ({
              "data-sse": ""
              "data-init": ("@get('" + ($req | href "/sse/presence") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
              "data-signals": ({
                games: (if $empty { {} } else { $games_signal })
              } | to json --raw)
            }))
    })

    (route {method: GET path: "/my/games"} {|req ctx|
      # Your library. Session-required: no session = nothing to show
      # (visitors get a "start a game" prompt rather than someone
      # else's data). A legacy `player` cookie is one-shot claimed
      # into a session here.
      let session = resolve-session $req
      let games = if $session == null { [] } else {
        try { .cat -T $"player.($session.user_id).games" | reverse } catch { [] }
      }
      # One signal keyed by game id. Each card binds via data-attr to
      # $games[<id>] (WC board state, plus playedMs for the overlay).
      # Live SSE patches merge per-game updates into the same shape;
      # no HTML re-render needed for snapshot changes.
      let games_signal = $games | reduce -f {} {|f acc|
        let resumed = game-head $f.id
        let lmid = $resumed | get follow_from_id? | default $f.id
        let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
        let state = $resumed.state | state-for-wc | upsert playedMs $played_ms
        $acc | upsert $f.id $state
      }
      let body = ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
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
                "data-signals": ({
                  games: $games_signal
                } | to json --raw)
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
        ([
          (DIV {class: "page"}
            (breadcrumb
              --left [
                (A {href: ($req | href $"/by/($owner_id)")} $"by ($owner_short)")
                (SPAN {class: "sep"} "·")
                (A {class: "game-id" href: ($req | href $"/watch/($game_id)")} $game_id_short)
                (SPAN {class: "sep"} "·")
                (SPAN {class: "game-presence"
                       "data-text": $"\($presence.byGame['($game_id)'] || 0) + ' here'"} "")
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
              }
                (DIV {id: "board-wrap"}
                  (render-tag "game-board" {"data-attr:state": "JSON.stringify($boardState)"})))))
        ] | layout $req $REV $DATASTAR_JS_PATH
              --title $"watching ($game_id_short) -- nu2048"
              --body-class "watch"
              # Signals must live on <body> (or an ancestor of any
              # data-text consumer) -- Datastar's DOM walk processes
              # attributes top-down, so the site-header's
              # `data-text="$presence.totalTabs"` evaluates BEFORE any
              # inner data-signals declaration takes effect.
              --body-attrs {
                "data-game-id": $game_id
                "data-signals": "{boardState: {tiles: [], gameOver: false}, score: 0, gameStatus: ''}"
              }
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
        let resumed = game-head $f.id
        let lmid = $resumed | get follow_from_id? | default $f.id
        let played_ms = (.id unpack $lmid | get timestamp | into int) / 1_000_000 | into int
        let state = $resumed.state | state-for-wc | upsert playedMs $played_ms
        $acc | upsert $f.id $state
      }
      ([
        (DIV {class: "page"}
          (breadcrumb
            --left [
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
            --sse true
            --body-attrs {
              "data-sse": ""
              "data-init": ("@get('" + ($req | href "/sse/presence") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
              "data-signals": ({
                games: $games_signal
              } | to json --raw)
            })
    })

    (route {method: GET path-matches: "/play/:game_id"} {|req ctx|
      let game_id = $ctx.game_id
      # Owner-or-404. Anonymous visitors and visitors whose session
      # doesn't own this game get a not-found -- /watch/<game_id> is
      # the public read-only path.
      let session = resolve-session $req
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
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      let game_id_short = $game_id | str substring 0..7
      ([
        (DIV {class: "page"}
          # The SSE pipeline emits a $lastReqId signal patch for every
          # move (no-op echo or state-changing snapshot). This effect
          # bridges that signal into window.onAck (script.js), which
          # clears the pending edge-line + records the RTT readout
          # iff the reqId matches the pending probe. data-on-signal-
          # patch only fires on signal patches (not on mount), so the
          # deferred-script-load timing is safe.
          # Guard window.onAck: Datastar fires data-on-signal-patch on the
          # initial signal merge, which can land before script.js's defer
          # has executed. Without the typeof check the page logs an
          # ExecuteExpression error on first paint (caught by test.mjs's
          # `no JS errors on /play load` assertion).
          # Short-circuit form is required (Datastar wraps the value in
          # `return (...)`, so `if` statements don't parse). When
          # window.onAck isn't yet defined (e.g. the first signals
          # merge lands before script.js's defer fires), the && yields
          # the undefined-ish left side without throwing.
          (DIV {"data-on-signal-patch": "window.onAck && window.onAck($lastReqId)" hidden: ""})
          # Breadcrumb header: left = path (game-id + live presence),
          # right = top-level actions. Home is the nu2048 title in the
          # site-header now, so no [esc] crumb here. The game-id is a
          # self-link so it can be right-clicked to copy a bookmarkable URL.
          (breadcrumb
            --left [
              (A {class: "game-id" href: ($req | href $"/play/($game_id)")} $game_id_short)
              (SPAN {class: "sep"} "·")
              (SPAN {class: "game-presence"
                     "data-text": $"\($presence.byGame['($game_id)'] || 0) + ' here'"} "")
            ]
            --right [
              # Same game, spectator view -- right-click to share.
              (A {class: "spectate-link" href: ($req | href $"/watch/($game_id)")} "watch")
              # Undo lives here (a meta action) rather than on the thumb
              # pad, so it can't be hit mid-play. Fires move("undo") via
              # the [data-intent] click delegate.
              (kbd-btn "u" --suffix "ndo" --intent "undo")
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
              # #board-wrap is the target for the data-pending edge-
              # line indicator script.js sets on keydown (cleared when
              # the move's $lastReqId comes back). The board itself is
              # the WC; it owns tile animations and the won/over badge
              # inside its shadow DOM.
              (DIV {id: "board-wrap"}
                (render-tag "game-board" {"data-attr:state": "JSON.stringify($boardState)"})))
            # Thumb-ergonomic cross D-pad. Each key is a <button
            # data-intent> wired through script.js's click delegate.
            # See `control-pad` in render.nu.
            (control-pad))
        )
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, encapsulated board web component."
            --body-class "play"
            --sse true
            --show-rtt true
            # iconify-icon web component for the D-pad arrow glyphs
            --head-extra [(SCRIPT-ICONIFY)]
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
