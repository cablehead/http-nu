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

const SCRIPT_DIR = path self | path dirname
const STATIC_DIR = $SCRIPT_DIR | path join "static"
# Cache-buster for static assets: fresh per server start, stable within one
# session. Browsers cache /styles.css?v=<REV> across page loads but refetch
# on the next server restart.
let REV = random uuid | str substring 0..7

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

# --- routes ---------------------------------------------------------------

{|req|
  dispatch $req [
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
      let cookies = $req | cookie parse
      let player_id = $cookies | get player? | default ""
      let games_topic = $"player.($player_id).games"
      if ($player_id | is-empty) {
        null | metadata set { merge {'http.response': {status: 400}} }
      } else {
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
      # Mint a games_topic frame for this player and 302 to /play/<id>.
      # Cookie minted on first visit.
      let cookies = $req | cookie parse
      let prior = $cookies | get player? | default ""
      let player_id = if ($prior | is-empty) { random uuid } else { $prior }
      let games_topic = $"player.($player_id).games"
      let new_frame = (null | .append $games_topic)
      let location = ($req | href $"/play/($new_frame.id)")
      "" | metadata set { merge {'http.response': {status: 302 headers: {Location: $location}}} }
      | cookie set "player" $player_id --max-age 31536000 --no-secure
    })

    (route {method: GET path: "/"} {|req ctx|
      # Splash = past games + New game link. The play view lives at
      # /play/<game_id>. Mint a cookie on first visit so subsequent
      # actions (creating games, etc.) have a player identity.
      let cookies = $req | cookie parse
      let prior = $cookies | get player? | default ""
      let player_id = if ($prior | is-empty) { random uuid } else { $prior }
      let games_topic = $"player.($player_id).games"
      let games = (try { .cat -T $games_topic | reverse } catch { [] })
      let scheme = $req.headers
        | get x-forwarded-proto?
        | default (if ($HTTP_NU.tls? | default null) != null { "https" } else { "http" })
      let host = $req.headers | get host? | default "localhost"
      let og_image = $"($scheme)://($host)" + ($req | href "/og.png")
      ([
        (DIV {class: "page"}
          (HEADER {class: "play-header"}
            (DIV {class: "left"}
              (SPAN {class: "page-title"} "past games"))
            (DIV {class: "right"}
              (A {class: "new-game" href: ($req | href "/new")} "+ new game")))
          # Always render .games-list (even if empty) so the SSE handler
          # has a stable target to prepend new-game cards into. The hint
          # below is a sibling, hidden via CSS when .games-list has any
          # children.
          (DIV {class: "games-list"} ($games | each {|f| render-game-card $req $f }))
          (P {class: "hint empty-state"} "no games yet."))
      ] | layout $req $REV $DATASTAR_JS_PATH
            --title "nu2048"
            --og-image $og_image
            --og-description "Event-sourced 2048 on http-nu: cross.stream snapshots, Datastar SSE, view-transition tile slides."
            --body-class "games-view"
            --body-attrs {
              "data-sse": ""
              "data-init": ("@get('" + ($req | href "/sse/games") + "', {retry: 'always', retryInterval: 1000, retryMaxCount: Infinity})")
            }
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })

    (route {method: GET path-matches: "/play/:game_id"} {|req ctx|
      let player_id = ($req | cookie parse | get player? | default (random uuid))
      let game_id = $ctx.game_id
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
          # Breadcrumb: "/" links back to splash, then a short game-id slug.
          (NAV {class: "breadcrumb"}
            (A {href: $home_href} "/")
            (SPAN {class: "sep"} "·")
            (SPAN {class: "game-id"} $game_id_short))
          (DIV {class: "play-layout"}
            (DIV {class: "board-controls"}
              (DIV {class: "score-block"}
                (render-score 0)
                (render-state-badge false false)))
            # data-init lives on .column (which is never patched) so the SSE
            # fetch + connection signal survive the morph of #game's contents.
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
                (BUTTON {type: "button" "data-intent": "h" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "h") (SPAN {class: "bracket"} "]"))
                (BUTTON {type: "button" "data-intent": "h" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "←") (SPAN {class: "bracket"} "]")))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "down")
                (BUTTON {type: "button" "data-intent": "j" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "j") (SPAN {class: "bracket"} "]"))
                (BUTTON {type: "button" "data-intent": "j" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "↓") (SPAN {class: "bracket"} "]")))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "up")
                (BUTTON {type: "button" "data-intent": "k" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "k") (SPAN {class: "bracket"} "]"))
                (BUTTON {type: "button" "data-intent": "k" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "↑") (SPAN {class: "bracket"} "]")))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "right")
                (BUTTON {type: "button" "data-intent": "l" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "l") (SPAN {class: "bracket"} "]"))
                (BUTTON {type: "button" "data-intent": "l" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "→") (SPAN {class: "bracket"} "]")))
              (DIV {class: "help-row"}
                (SPAN {class: "label"} "undo")
                (BUTTON {type: "button" "data-intent": "undo" class: "kbd-btn"}
                  (SPAN {class: "bracket"} "[") (SPAN {class: "key"} "u") (SPAN {class: "bracket"} "]"))
                (SPAN {}))
              (DIV {class: "help-row help-fx"}
                (SPAN {class: "label"} "tuner")
                (BUTTON {type: "button" class: "kbd-btn fx-toggle"} "fx")
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
      | cookie set "player" $player_id --max-age 31536000 --no-secure)
    })
  ]
}
