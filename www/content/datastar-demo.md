Adapted from [data-star.dev](https://data-star.dev)'s intro example. Client-side: `data-signals` tracks `running` state. On click, `running=true` disables the button via `data-attr:disabled` and `data-class`.

Server-side: `from datastar-request` extracts signals (like interval). `generate` streams values while maintaining state via its accumulator, building up the message character by character. Each iteration pipes through `to dstar-patch-element` for DOM updates.

Finally, `append` adds a `to dstar-patch-signal` to set `running=false`, re-enabling the button when done. `to sse` handles Content-Type and formats everything as server-sent events.
