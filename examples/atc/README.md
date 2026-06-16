# atc -- fan-in over live air traffic

A one-file http-nu app that demonstrates **fan-in**: many independent source
streams merged into one consumer. It's framed as a **Heathrow approach radar**
-- live arrivals split into two sectors, where each air traffic controller's
scope is the live merge of the sectors they're watching.

```sh
cd examples/atc
http-nu --dev --datastar --watch --store ./store :3017 serve.nu
```

Open <http://localhost:3017>. Pick **North**, **South**, or **Both**. North and
South each show one half of the approach; Both fans the two together into the
single picture a "final director" works from.

- `--datastar` serves the Datastar runtime at `/datastar@1.0.2.js`.
- `--store ./store` gives the app a [cross.stream](https://github.com/cablehead/xs)
  store, so `.append` / `.cat` / `.last` are in scope. The store is runtime-only
  here (every frame is `ttl last:1`, the poller refills it), so it's gitignored.
- `--watch` hot-reloads `serve.nu` on save.

## The idea

In real ATC, airspace is carved into **sectors**, and a controller *is* a
sector: an aircraft is theirs while it's inside their volume, and gets **handed
off** to the next controller as it crosses a boundary. A busy approach is split
between several controllers; combine their sectors and you get the whole
approach circle back.

That maps onto stream primitives exactly:

| ATC | this app |
| --- | --- |
| a sector | a topic, `feed.sector.<name>` |
| the planes a controller is working | the aircraft in that sector's feed |
| a controller's radar scope | a `.cat --follow` over the sector feed(s) |
| watching more than one sector | `interleave` of several feeds = **fan-in** |
| a handoff | a plane moving from one feed to the other |

## Data flow

```
adsb.lol  ──poll 2s──▶  assign N/S  ──▶  feed.sector.north ─┐
(live ADS-B)                             feed.sector.south ─┤
                                                            ├─▶  /scope SSE
                                          $sel picks which ─┘     (interleave)
                                                                    │
                                                          datastar-patch-elements
                                                                    │
                                                              the radar scope
```

## Walking through `serve.nu`

### 1. The datasource: a poller turning ADS-B into two sector feeds

[`serve.nu` L17-44](serve.nu#L17-L44). On startup (guarded on `--store`) a
background [`job spawn`](https://www.nushell.sh/commands/docs/job_spawn.html)
loop polls the free, no-auth [adsb.lol](https://api.adsb.lol) aggregator every
2s for aircraft within 45nm of Heathrow.

- **arrivals filter** ([L34-35](serve.nu#L34-L35)): keep only numeric altitudes
  below 13,000ft that aren't climbing (`baro_rate < 200`), so overflights and
  departures drop out and every blip means "descending toward the field."
- **the split** ([L36-37](serve.nu#L36-L37)): one line of latitude through
  Heathrow divides arrivals into `north` and `south`.
- **the feeds** ([L38-39](serve.nu#L38-L39)): each half is written to its own
  topic with `.append ... --ttl last:1`, so each feed topic holds exactly one
  frame -- the current state of that sector. `last:1` means the store
  garbage-collects the previous frame as each new one lands (see the
  [TTL reference](https://cablehead.github.io/xs/reference/ttl/)).

The two feeds are the **independent sources**. Nothing here knows or cares who's
listening.

### 2. The fan-in: the `/scope` SSE route

[`serve.nu` L122-134](serve.nu#L122-L134). This is the consumer, one controller's
scope.

- **selection** ([L123](serve.nu#L123)): `from datastar-signals $req` reads the
  client's `$sel` signal (Datastar sends signals as the `datastar` query param on
  a GET). `sectors-for` ([L65-67](serve.nu#L65-L67)) turns it into a list of
  sector names.
- **one sector** ([L125-129](serve.nu#L125-L129)): a plain
  `.cat --last 1 --follow` over that sector's feed -- `--last 1` paints the
  current state immediately on connect, `--follow` streams every update after.
- **fan-in** ([L130-134](serve.nu#L130-L134)): for *Both*,
  [`interleave`](https://www.nushell.sh/commands/docs/interleave.html) merges the
  two `.cat --follow` streams into one. **This is the whole point** -- two source
  feeds become one stream the scope reads through a single connection. (Note the
  closures `{|| .cat ... }`: never bind a `--follow` stream to `let`, it collects
  and hangs. Pipe it straight in.)

Each wake re-renders and patches `#scope` via
[`to datastar-patch-elements`](../../src/stdlib/datastar/mod.nu) then
[`to sse`](../../src/stdlib/datastar/mod.nu).

### 3. Rendering: feeds to a round scope

[`render-scope` L69-78](serve.nu#L69-L78) reads the current head of each selected
feed (`.last "feed.sector.<s>"`) and draws the merged set of aircraft.
[`blip` L52-63](serve.nu#L52-L63) places each one by **bearing + range from
Heathrow** (longitude squeezed by `cos(lat)` so the scope stays round), which is
why the layout reads as a radar centered on the airport rather than a flat map.
Each blip's `<b>` is the callsign and `<i>` is the altitude in feet.

Every element carries a **stable `id`** -- the furniture (`#ring1`, `#hsplit`,
...) and one blip per aircraft (`id="ac-<hex>"`). The default
`datastar-patch-elements` mode is *morph*, which matches by id, so each plane is
updated **in place** across the 2s patches instead of its DOM node being torn
down and rebuilt. Without ids you can't even select/copy a callsign, the morph
replaces it every tick.

### 4. The client: Datastar, no JS

[`page` L80-118](serve.nu#L80-L118) is static HTML. The reactivity is
[Datastar](https://data-star.dev/docs) data-attributes:

- [`data-signals` L106](serve.nu#L106): seeds `$sel = "both"`.
- [`data-effect` L114](serve.nu#L114): `"$sel; @get('/scope')"` -- runs on load
  **and whenever `$sel` changes**, re-opening the SSE for the newly selected
  sector. This is the "subscribe to the channel(s) I'm responsible for" action,
  driven entirely by one signal.
- [buttons L110-112](serve.nu#L110-L112): `data-on:click` just sets `$sel`;
  `data-class:active` highlights the current one.

Responses are processed by content-type: the page is `text/html`, `/scope` is
`text/event-stream` carrying
[`datastar-patch-elements`](https://data-star.dev/reference/sse_events) events
that morph `#scope` by id.

## What it fakes (so you don't over-trust it)

- **Snapshot, not smooth.** Blips jump every 2s; there's no interpolation.
- **The split is a clean diameter**, not the real geometry. Heathrow's arrivals
  actually funnel through four corner [holding stacks](https://en.wikipedia.org/wiki/Heathrow_arrival_stacks)
  (Bovingdon N, Lambourne NE, Ockham S, Biggin SE); "North/South" groups them
  loosely.
- **No altitude dimension.** Real sectors are 3D (a low and a high controller can
  share the same footprint); this splits laterally only.
- **"Arrival" is a heuristic** (low + not climbing), not the actual route/STAR
  assignment.

## Where to take it next

- **Handoff**: animate a blip crossing the line, visibly leaving one feed and
  joining the other.
- **Four stacks**: swap the latitude split for assign-by-bearing wedges, four
  feeder channels into one final-director fan-in.
- **Altitude band**: add a vertical split so "yours" means lateral *and*
  vertical.
- **Exclusive ownership**: let two browsers each claim sectors, so claiming one
  is a controller-to-controller handoff.
