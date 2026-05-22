# Public CLI-friendly surface: pure logic + store helpers. Designed for
#
#   $env.XS_ADDR = (realpath ./store)
#   use examples/2048/tfe *
#   list-games
#
# The server-side modules (render.nu, sse.nu) are NOT re-exported here.
# They depend on http-nu commands (.mj, datastar, html DSL) that aren't
# available in a plain nu shell, and their `export-env` blocks only
# trigger on direct `use module/sub.nu` (not via `export use` here).
# serve.nu imports them directly.
#
# Files in this directory:
#   game.nu             pure game logic, no deps         (re-exported)
#   store.nu            .cat/.last/.append helpers       (re-exported)
#   render.nu           HTML output (server only)        (NOT re-exported)
#   sse.nu              SSE pipeline (server only)       (NOT re-exported)
#   snapshot-actor.nu   xs actor source (registered, not imported)

export use ./game.nu *
export use ./store.nu *
