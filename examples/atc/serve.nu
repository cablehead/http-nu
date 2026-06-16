use http-nu/router *
use http-nu/datastar *
use http-nu/http *

# Heathrow approach: a round radar scope centered on LHR, arrivals only,
# split into two half-disk sectors (North / South) by an east-west line
# through the field. Each sector is a "channel"; Both = the full circle
# (the final-director fan-in of both halves).

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

# one arrival -> a blip placed by bearing+range from Heathrow
def blip [a: record]: nothing -> string {
  let dx = (($a.lon - $LHR_LON) * $COSLAT)
  let dy = ($a.lat - $LHR_LAT)
  let x = (clamp (50 + ($dx / $RANGE_DEG) * 50))
  let y = (clamp (50 - ($dy / $RANGE_DEG) * 50))
  let label = if (($a.flight | default "") | is-empty) { $a.hex } else { $a.flight }
  # Stable id keyed on the ICAO hex so the patch MORPHS each plane in place
  # (position/text updated) instead of tearing down and rebuilding the blip.
  # Without it, every 2s patch wipes the subtree and you lose text selection.
  let key = if (($a.hex | default "") | is-empty) { ($label | str replace --all " " "") } else { $a.hex }
  # vertical trend from baro_rate (fpm): descending arrivals pulse so it's
  # obvious the altitude is live and dropping.
  let tr = if $a.vs < -100 { "down" } else if $a.vs > 100 { "up" } else { "level" }
  let ar = if $tr == "down" { "v" } else if $tr == "up" { "^" } else { "=" }
  $'<div id="ac-($key)" class="ac" style="left:($x)%;top:($y)%"><span class="cs">($label)</span><span class="alt"><span class="num">($a.alt)</span><span class="u"> ft</span> <span class="tr ($tr)">($ar)</span></span></div>'
}

def sectors-for [sel: string]: nothing -> list {
  match $sel { "north" => [north], "south" => [south], _ => [north south] }
}

def render-scope [sectors: list]: nothing -> string {
  let acs = ($sectors | each {|s|
    .last $"feed.sector.($s)" | default {} | get meta?.aircraft? | default []
  } | flatten)
  let blips = ($acs | each {|a| blip $a } | str join "\n")
  let title = ($sectors | str join "+")
  # Every child carries a stable id so morphing matches them across patches
  # (the static furniture stays put; blips update in place).
  $'<div id="scope" class="scope"><div id="ring2" class="ring ring2"></div><div id="ring1" class="ring ring1"></div><div id="hsplit" class="hsplit"></div><div id="labN" class="lab labN">N</div><div id="labS" class="lab labS">S</div><div id="lhr" class="lhr"></div><div id="meta" class="meta">($title) -- ($acs | length) arrivals</div>($blips)</div>'
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
 .meta{position:absolute;right:.6rem;top:.5rem;color:#5a5;font-size:.7rem}
 .ac{position:absolute;transform:translate(-50%,-50%);font-size:.55rem;line-height:1.15;white-space:nowrap;text-align:center}
 .ac .cs{display:block;color:#9ef;font-weight:700;letter-spacing:.02em}
 .ac .alt{display:block;font-size:.5rem}
 .ac .num{color:#fc6;font-weight:600}
 .ac .u{color:#a85;font-size:.42rem}
 .ac .tr{font-weight:700}
 .ac .tr.down{color:#fc6;animation:trpulse 1.3s ease-in-out infinite}
 .ac .tr.up{color:#6f6}
 .ac .tr.level{color:#7a7}
 @keyframes trpulse{0%,100%{opacity:.3}50%{opacity:1}}
</style></head>
<body data-signals='{"sel":"both"}'>
 <h1>Fan-in &mdash; live Heathrow arrivals</h1>
 <p class="lede">A controller's scope is the live <b>merge of the sector feeds it subscribes to</b>. North and South are two independent feeds (planes hand off between them at the line); <b>Both</b> fans them into one stream &mdash; the final director's view.</p>
 <div class="sel">
  <button data-on:click="$sel = 'north'" data-class:active="$sel == 'north'">North</button>
  <button data-on:click="$sel = 'south'" data-class:active="$sel == 'south'">South</button>
  <button data-on:click="$sel = 'both'" data-class:active="$sel == 'both'">Both (final director)</button>
 </div>
 <div data-effect="$sel; @get('/scope')">
  <div id="scope" class="scope"><div class="meta">connecting&hellip;</div></div>
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
    (route {method: GET path: "/"} {|req ctx|
      page | metadata set { merge {'http.response': {headers: {"content-type": "text/html; charset=utf-8"}}} }
    })
  ]
}
