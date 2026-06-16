use http-nu/router *
use http-nu/datastar *
use http-nu/http *

# Heathrow approach: a round radar scope centered on LHR, arrivals only,
# split into two half-disk sectors (North / South) by an east-west line
# through the field. Each sector is a "channel"; Both = the full circle
# (the final-director fan-in of both halves). Click a blip for a detail card.

const LHR_LAT = 51.4775
const LHR_LON = -0.4614
const RANGE_DEG = 0.75            # scope half-extent (~45 nm) in degrees lat
const COSLAT = 0.623             # cos(51.48 deg): squeeze lon into lat-scale
const ALT_CEIL = 13000           # terminal/arrival ceiling (ft)
const POLL_URL = "https://api.adsb.lol/v2/lat/51.4775/lon/-0.4614/dist/45"

# --- poller: live ADS-B -> arrivals, split N/S into two sector feeds
if ($HTTP_NU.store? | default null) != null {
  job spawn {
    loop {
      try {
        let r = (http get --headers {User-Agent: "atc-fanin-demo/0.1"} $POLL_URL)
        let acs = ($r.ac | default []
          | where {|a| (($a.lat? | default null) != null) and (($a.lon? | default null) != null) }
          | each {|a| {
              hex: ($a.hex? | default "")
              flight: ($a.flight? | default "" | str trim)
              lat: $a.lat
              lon: $a.lon
              alt: ($a.alt_baro? | default 0)
              gs: ($a.gs? | default 0)
              vs: ($a.baro_rate? | default 0)
              reg: ($a.r? | default "")
              type: ($a.t? | default "")
              track: ($a.track? | default 0)
              squawk: ($a.squawk? | default "")
              cat: ($a.category? | default "")
              dst: ($a.dst? | default 0)
              dir: ($a.dir? | default 0)
            }}
          # arrivals-ish: numeric altitude below the ceiling, not climbing
          | where {|a| (($a.alt | describe) != "string") and ($a.alt < $ALT_CEIL) and ($a.vs < 200) })
        let north = ($acs | where lat >= $LHR_LAT)
        let south = ($acs | where lat < $LHR_LAT)
        null | .append "feed.sector.north" --ttl last:1 --meta {aircraft: $north, n: ($north | length)}
        null | .append "feed.sector.south" --ttl last:1 --meta {aircraft: $south, n: ($south | length)}
      } catch {|e| print $"poll error: ($e.msg)" }
      sleep 2sec
    }
  }
}

def clamp [v: float]: nothing -> float {
  let a = ([$v 0.0] | math max)
  [$a 100.0] | math min
}

# one arrival -> a clickable blip placed by bearing+range from Heathrow
def blip [a: record]: nothing -> string {
  let dx = (($a.lon - $LHR_LON) * $COSLAT)
  let dy = ($a.lat - $LHR_LAT)
  let x = (clamp (50 + ($dx / $RANGE_DEG) * 50))
  let y = (clamp (50 - ($dy / $RANGE_DEG) * 50))
  let label = if (($a.flight | default "") | is-empty) { $a.hex } else { $a.flight }
  # Stable id keyed on the ICAO hex so the patch MORPHS each plane in place
  # (position/text updated) instead of tearing down and rebuilding the blip.
  let key = if (($a.hex | default "") | is-empty) { ($label | str replace --all " " "") } else { $a.hex }
  # vertical trend from baro_rate (fpm): descending arrivals pulse so it's
  # obvious the altitude is live and dropping.
  let tr = if $a.vs < -100 { "down" } else if $a.vs > 100 { "up" } else { "level" }
  let ar = if $tr == "down" { "v" } else if $tr == "up" { "^" } else { "=" }
  # click highlights this blip, records cursor, drops any open card, and asks
  # the server to append a freshly-rendered one. Dropping the old card first
  # means no stale frame while the new one is in flight.
  let click = ("$ac = '" + $key + "'; $cx = Math.min(evt.clientX, innerWidth - 270); $cy = Math.min(evt.clientY, innerHeight - 230); document.getElementById('card')?.remove(); @get('/flight/" + $key + "')")
  let issel = $"$ac == '($key)'"
  # dot + callsign always; altitude/trend revealed on hover or when selected
  $'<div id="ac-($key)" class="ac" data-on:click__stop="($click)" data-class:picked="($issel)" style="left:($x)%;top:($y)%"><span class="dot"></span><span class="lbl"><span class="cs">($label)</span><span class="alt"><span class="num">($a.alt)</span><span class="u"> ft</span> <span class="tr ($tr)">($ar)</span></span></span></div>'
}

def sectors-for [sel: string]: nothing -> list {
  match $sel { "north" => [north], "south" => [south], _ => [north south] }
}

# read the current aircraft across the given sectors (feed heads)
def aircraft-in [sectors: list]: nothing -> list {
  $sectors | each {|s|
    .last $"feed.sector.($s)" | default {} | get meta?.aircraft? | default []
  } | flatten
}

def render-scope [sectors: list]: nothing -> string {
  # per-sector aircraft, so the Both view can show the N/S breakdown
  let per = ($sectors | each {|s|
    {s: $s, acs: (.last $"feed.sector.($s)" | default {} | get meta?.aircraft? | default [])}
  })
  let acs = ($per | get acs | flatten)
  let blips = ($acs | each {|a| blip $a } | str join "\n")
  let count = if ($per | length) > 1 {
    (($per | each {|p| $"($p.s | str capitalize) ($p.acs | length)"} | str join " + ") + $" = ($acs | length) tracked")
  } else {
    $"($per | first | get s | str capitalize) -- ($acs | length) tracked"
  }
  # Two top-level elements patched by id: the count line (outside the circular
  # scope, which clips its corners) and the scope itself.
  $'<div id="count" class="count">($count)</div>
<div id="scope" class="scope"><div id="ring2" class="ring ring2"></div><div id="ring1" class="ring ring1"></div><div id="hsplit" class="hsplit"></div><div id="labN" class="lab labN">N</div><div id="labS" class="lab labS">S</div><div id="lhr" class="lhr"></div>($blips)</div>'
}

# detail card for one aircraft. Server-rendered HTML, appended to <body> on
# click and positioned at the cursor; removed from the DOM directly (no server
# round-trip) on close or outside-click.
def render-card [a: any, hex: string, left: int, top: int]: nothing -> string {
  let style = $"left:($left)px;top:($top)px"
  let close = "$ac = ''; document.getElementById('card')?.remove()"
  if ($a == null) {
    return $'<div id="card" class="card" style="($style)" data-on:click__stop="true"><div class="chead"><b>($hex)</b><button class="x" data-on:click__stop="($close)">close</button></div><div class="row">out of range now</div></div>'
  }
  let cs = if (($a.flight | default "") | is-empty) { $hex } else { $a.flight }
  let vs = ($a.vs | default 0)
  let trend = if $vs < -100 { $"descending ($vs | math abs) fpm" } else if $vs > 100 { $"climbing ($vs) fpm" } else { "level" }
  let rows = ([
    ["reg" ($a.reg | default "-")]
    ["type" ($a.type | default "-")]
    ["category" ($a.cat | default "-")]
    ["altitude" $"($a.alt) ft (($trend))"]
    ["ground speed" $"($a.gs | math round) kt"]
    ["track" $"($a.track | math round) deg"]
    ["from LHR" $"($a.dst | math round --precision 1) nm, brg ($a.dir | math round) deg"]
    ["squawk" ($a.squawk | default "-")]
  ] | each {|r| $'<div class="row"><span class="k">($r.0)</span><span class="v">($r.1)</span></div>' } | str join)
  $'<div id="card" class="card" style="($style)" data-on:click__stop="true"><div class="chead"><b>($cs)</b><button class="x" data-on:click__stop="($close)">close</button></div>($rows)</div>'
}

def page []: nothing -> string {
  r#'<!doctype html><html><head><meta charset="utf-8">
<title>Heathrow approach -- fan-in slice</title>
<script type="module" src="/datastar@1.0.2.js"></script>
<style>
 body{background:#0a0f0a;color:#7CFC7C;font-family:ui-monospace,monospace;margin:0;padding:1rem;text-align:center}
 h1{font-size:1rem;font-weight:normal;margin:.2rem 0}
 .lede{color:#5a8a5a;font-size:.78rem;max-width:60ch;margin:.2rem auto .4rem;line-height:1.35}
 .lede b{color:#9ec;font-weight:600}
 .sel{margin:.5rem 0}
 button{background:#06120a;color:#7CFC7C;border:1px solid #1c3;font-family:inherit;padding:.3rem .9rem;cursor:pointer;margin:0 .15rem}
 button.active{background:#1c3;color:#021}
 .scope{position:relative;width:80vmin;height:80vmin;border-radius:50%;border:1px solid #1c3;background:#06120a;overflow:hidden;margin:.5rem auto}
 .ring{position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);border:1px solid #103d22;border-radius:50%}
 .ring1{width:50%;height:50%}
 .ring2{width:78%;height:78%}
 .hsplit{position:absolute;left:0;right:0;top:50%;border-top:1px dashed #2a5}
 .lab{position:absolute;left:50%;transform:translateX(-50%);color:#3a6;font-size:.7rem}
 .labN{top:.3rem}
 .labS{bottom:.3rem}
 .lhr{position:absolute;left:50%;top:50%;width:7px;height:7px;background:#fe6;border-radius:50%;transform:translate(-50%,-50%)}
 .count{color:#9c9;font-size:.85rem;margin:.3rem auto .1rem;min-height:1.1em}
 .ac{position:absolute;cursor:pointer;z-index:1}
 .ac:hover,.ac.picked{z-index:3}
 .ac .dot{position:absolute;left:0;top:0;transform:translate(-50%,-50%);width:6px;height:6px;border-radius:50%;background:#9ef;box-shadow:0 0 3px #9ef9}
 .ac .lbl{position:absolute;left:0;top:5px;transform:translateX(-50%);font-size:.5rem;line-height:1.1;white-space:nowrap;text-align:center}
 .ac .cs{display:block;color:#9ef;font-weight:700;letter-spacing:.02em}
 .ac .alt{display:block;font-size:.5rem}
 .ac:hover .lbl,.ac.picked .lbl{background:#06120af2;border-radius:2px;box-shadow:0 0 0 3px #06120af2,0 0 0 4px #1c3}
 .ac.picked .dot{background:#fc6;box-shadow:0 0 6px #fc6}
 .ac .num{color:#fc6;font-weight:600}
 .ac .u{color:#a85;font-size:.42rem}
 .ac .tr{font-weight:700}
 .ac .tr.down{color:#fc6;animation:trpulse 1.3s ease-in-out infinite}
 .ac .tr.up{color:#6f6}
 .ac .tr.level{color:#7a7}
 @keyframes trpulse{0%,100%{opacity:.3}50%{opacity:1}}
 .card{position:fixed;z-index:5;text-align:left;background:#0b1a10;border:1px solid #1c3;padding:.55rem .8rem;min-width:230px;width:max-content;max-width:360px;font-size:.72rem;box-shadow:0 2px 12px #000a}
 .chead{display:flex;justify-content:space-between;align-items:center;gap:1rem;margin-bottom:.4rem}
 .chead b{color:#9ef;font-size:.95rem}
 .card .x{font-size:.58rem;padding:.1rem .4rem;margin:0;color:#7a7}
 .row{display:flex;flex-wrap:nowrap;justify-content:space-between;gap:1.5rem;padding:.08rem 0}
 .row .k{color:#5a8a5a;white-space:nowrap}
 .row .v{color:#cfe;white-space:nowrap}
</style></head>
<body data-signals='{"sel":"both","ac":"","cx":0,"cy":0}' data-on:click__window="$ac = ''; document.getElementById('card')?.remove()">
 <h1>Fan-in &mdash; live Heathrow arrivals</h1>
 <p class="lede">A controller's scope is the live merge of the <b>sector feeds</b> it subscribes to. <b>North</b> and <b>South</b> are two independent feeds; planes <b>hand off</b> between them at the line. <b>Both</b> fans them into one stream, the <b>final director</b>'s view. Click a plane for detail.</p>
 <div class="sel">
  <button data-on:click="$sel = 'north'" data-class:active="$sel == 'north'">North</button>
  <button data-on:click="$sel = 'south'" data-class:active="$sel == 'south'">South</button>
  <button data-on:click="$sel = 'both'" data-class:active="$sel == 'both'">Both (final director)</button>
 </div>
 <div id="count" class="count">connecting&hellip;</div>
 <div data-effect="$sel; @get('/scope')">
  <div id="scope" class="scope"></div>
 </div>
</body></html>'#
}

{|req|
  dispatch $req [
    (route {method: GET path: "/scope"} {|req ctx|
      let sel = ("" | from datastar-signals $req | get sel? | default "both")
      let sectors = (sectors-for $sel)
      if ($sectors | length) == 1 {
        let s = ($sectors | first)
        .cat --last 1 --follow -T $"feed.sector.($s)"
        | each {|f| render-scope $sectors | to datastar-patch-elements }
        | to sse
      } else {
        null | interleave {|| .cat --last 1 --follow -T "feed.sector.north" } {|| .cat --last 1 --follow -T "feed.sector.south" }
        | each {|f| render-scope $sectors | to datastar-patch-elements }
        | to sse
      }
    })
    (route {method: GET path-matches: "/flight/:hex"} {|req ctx|
      let sig = ("" | from datastar-signals $req)
      let left = (($sig | get cx? | default 0 | into int) + 14)
      let top = (($sig | get cy? | default 0 | into int) - 10)
      let hex = $ctx.hex
      let match = (aircraft-in [north south] | where hex == $hex)
      let a = if ($match | is-empty) { null } else { $match | first }
      render-card $a $hex $left $top
      | to datastar-patch-elements --selector "body" --mode append
      | to sse
    })
    (route {method: GET path: "/"} {|req ctx|
      page | metadata set { merge {'http.response': {headers: {"content-type": "text/html; charset=utf-8"}}} }
    })
  ]
}
