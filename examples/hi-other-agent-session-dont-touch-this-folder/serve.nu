# Tile-palette swatches: five color-theory progressions for the 2048 value
# ladder (2 -> 2048). Numbers double exponentially; each row answers "what
# does the color do?" differently.
#
# Run standalone:
#   cat examples/hi-other-agent-session-dont-touch-this-folder/serve.nu | http-nu :3001 -

const VALUES = [2 4 8 16 32 64 128 256 512 1024 2048 4096 8192]

# Palettes return list<{bg, fg}>. Text contrast is computed per-tile so each
# progression can pick legible text against its own backgrounds.

# String-interp wrapping `$"oklch(($a) ($b) ...)"` confuses nu's parser
# (adjacent `($x) ($y)` looks like function application). Pre-join the args.
def oklch [l: float, c: float, h: float]: nothing -> string {
  let args = [$l $c $h] | str join ' '
  'oklch(' + $args + ')'
}

# Pick legible text given background OKLCH lightness.
def fg-for-l [l: float]: nothing -> string {
  if $l > 0.62 { "#776e65" } else { "#f9f6f2" }
}

# Original Cirulli palette. Past 2048 uses a single `.tile-super` class
# (#3c3a32 -- dark warm near-black) -- the gold ramp intentionally breaks
# to signal "you've gone past the game's intended end." Cirulli's fg rule
# is value-based: 2/4 get dark text, everything else gets cream.
def cirulli []: nothing -> list {
  let bgs = ["#eee4da" "#ede0c8" "#f2b179" "#f59563" "#f67c5f" "#f65e3b"
             "#edcf72" "#edcc61" "#edc850" "#edc53f" "#edc22e" "#3c3a32" "#3c3a32"]
  $bgs | enumerate | each {|p|
    let v = $VALUES | get $p.index
    {bg: $p.item, fg: (if $v <= 4 { "#776e65" } else { "#f9f6f2" })}
  }
}

# Weber-Fechner honest: equal perceptual step per doubling.
def oklch-linear []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let l = 0.92 - 0.47 * $t
    {bg: (oklch $l (0.05 + 0.13 * $t) 75), fg: (fg-for-l $l)}
  }
}

# Lightness drifts gently; chroma climbs convex (t squared). Endgame radiates.
def chroma-blowout []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let l = 0.88 - 0.25 * $t
    {bg: (oklch $l (0.02 + 0.22 * ($t * $t)) 55), fg: (fg-for-l $l)}
  }
}

# Planckian locus: dim deep red -> red -> orange -> amber -> warm white ->
# white-hot. Hue stays in the warm half of the wheel (30 -> 90 only), chroma
# starts high and decays toward white as we approach incandescence.
# L ramps concave (fast then plateau) so the top tiles are near-white.
def blackbody []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let l = 0.42 + 0.55 * ($t ** 0.55)
    let c = 0.03 + 0.20 * ((1 - $t) ** 1.2)
    let h = 28 + 62 * ($t ** 0.6)
    {bg: (oklch $l $c $h), fg: (fg-for-l $l)}
  }
}

# Sigmoid in lightness: flat plateau, plunge, flat dark plateau on top.
def sigmoid-light []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let x = 8 * ($t - 0.5)
    let sig = 1 / (1 + (2.718281828459045 ** (0 - $x)))
    let l = 0.93 - 0.55 * $sig
    {bg: (oklch $l 0.13 65), fg: (fg-for-l $l)}
  }
}

def row [name: string pairs: list note: string]: nothing -> string {
  let swatches = $pairs | enumerate | each {|p|
    let v = $VALUES | get $p.index
    $"<div class='sw' style='background: ($p.item.bg); color: ($p.item.fg)'>($v)</div>"
  } | str join ""
  $"<section><h3>($name)</h3><div class='row'>($swatches)</div><p class='note'>($note)</p></section>"
}

def page []: nothing -> string {
  let sections = [
    (row "Cirulli (original)" (cirulli)
      "Hand-tuned sigmoid. Beige plateau on 2-4, warm-red blowout through 8-64, clean linear gold ramp to 2048 -- then the ramp BREAKS: every super-tile (4096+) is the same #3c3a32 near-black. Past-the-end signaling.")
    (row "OKLCH linear" (oklch-linear)
      "Equal perceptual step per doubling (Weber-Fechner honest). Calm and legible, but 2048 doesn't feel like an event -- every tier weighs the same.")
    (row "Chroma blowout" (chroma-blowout)
      "Lightness drifts gently; chroma climbs convex. Late tiles radiate -- back-loaded drama. 1024 and 2048 feel almost incandescent against the muted low end.")
    (row "Blackbody march" (blackbody)
      "Dim deep red to white-hot, hue confined to the warm half of the wheel (28 -> 90). Chroma decays as lightness climbs -- saturated at the cool end, near-neutral when incandescent. Reads as a glowing element.")
    (row "Sigmoid lightness" (sigmoid-light)
      "Flat-plunge-flat. The 64-to-256 stretch is a regime change; the top tier is a dark slate plateau. Mid-game is where the work feels visible.")
  ] | str join "\n"
  # Static chrome uses a plain string (CSS `repeat(13, 1fr)` etc. would
  # confuse nu's interpolation parser).
  let head = "<!doctype html>
<html><head><meta charset='UTF-8'>
<title>tile palette experiments</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 920px; margin: 2rem auto; padding: 0 1rem; background: #faf8ef; color: #776e65; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  .lede { font-size: 13px; opacity: 0.7; margin: 0 0 1.5rem; }
  section { margin: 1.5rem 0; }
  h3 { font-size: 12px; margin: 0 0 6px; letter-spacing: 0.06em; text-transform: uppercase; }
  .row { display: grid; grid-template-columns: repeat(13, 1fr); gap: 4px; }
  .sw { aspect-ratio: 1; display: flex; align-items: center; justify-content: center; border-radius: 4px; font-weight: bold; font-size: clamp(10px, 1.3vw, 15px); }
  .note { font-size: 13px; margin: 8px 0 0; opacity: 0.85; line-height: 1.45; }
</style></head>
<body>
<h1>2048 tile palette experiments</h1>
<p class='lede'>Five color-theory progressions for the same value ladder. Numbers double; what does the color do?</p>
"
  $head + $sections + "
</body></html>"
}

{|req|
  match $req.path {
    "/" => { page | metadata set --content-type "text/html" }
    _ => { "404" | metadata set { merge {'http.response': {status: 404}} } }
  }
}
