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

# OKLCH -> linear sRGB -> WCAG relative luminance Y. Uses Björn Ottosson's
# OKLab matrices; out-of-gamut channels are clipped to [0,1] before the
# luminance sum, which is enough for our coarse pass/fail check.
def oklch-to-luminance [l: float, c: float, h: float]: nothing -> float {
  let hr = $h * 3.141592653589793 / 180
  let a = $c * ($hr | math cos)
  let b = $c * ($hr | math sin)
  let lp = $l + 0.3963377774 * $a + 0.2158037573 * $b
  let mp = $l - 0.1055613458 * $a - 0.0638541728 * $b
  let sp = $l - 0.0894841775 * $a - 1.2914855480 * $b
  let ll = $lp * $lp * $lp
  let mm = $mp * $mp * $mp
  let ss = $sp * $sp * $sp
  let r =  4.0767416621 * $ll - 3.3077115913 * $mm + 0.2309699292 * $ss
  let g = -1.2684380046 * $ll + 2.6097574011 * $mm - 0.3413193965 * $ss
  let b2 = -0.0041960863 * $ll - 0.7034186147 * $mm + 1.7076147010 * $ss
  let clamp = {|v| if $v < 0 { 0.0 } else if $v > 1 { 1.0 } else { $v } }
  0.2126 * (do $clamp $r) + 0.7152 * (do $clamp $g) + 0.0722 * (do $clamp $b2)
}

def wcag-ratio [y1: float, y2: float]: nothing -> float {
  let lo = if $y1 < $y2 { $y1 } else { $y2 }
  let hi = if $y1 < $y2 { $y2 } else { $y1 }
  ($hi + 0.05) / ($lo + 0.05)
}

# Two-step fg picker.
#
# Step 1 (propose, aesthetic): pick a fg in the SAME HUE FAMILY as the bg.
# Same H, low chroma (0.06), L on the opposite extreme of bg L. This keeps
# each tile's number tinted with its own color rather than bleached to pure
# white/black -- the "tonal in-hue" approach.
#
# Step 2 (verify, accessibility): compute WCAG 2 contrast ratio against the
# bg. If below 4.5:1 (AA body text), push L further toward 0 or 1 until it
# passes or we run out of room. So: in-hue is the proposal, WCAG is the gate.
def fg-pick [bg_l: float, bg_c: float, bg_h: float]: nothing -> string {
  let bg_y = oklch-to-luminance $bg_l $bg_c $bg_h
  let candidates = if $bg_l > 0.5 { [0.20 0.15 0.10 0.05] } else { [0.95 0.97 0.99 1.0] }
  mut picked = $candidates | last
  for lp in $candidates {
    let y = oklch-to-luminance $lp 0.06 $bg_h
    if (wcag-ratio $bg_y $y) >= 4.5 {
      $picked = $lp
      break
    }
  }
  oklch $picked 0.06 $bg_h
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
    let c = 0.05 + 0.13 * $t
    let h = 75.0
    {bg: (oklch $l $c $h), fg: (fg-pick $l $c $h)}
  }
}

# Lightness drifts gently; chroma climbs convex (t squared). Endgame radiates.
def chroma-blowout []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let l = 0.88 - 0.25 * $t
    let c = 0.02 + 0.22 * ($t * $t)
    let h = 55.0
    {bg: (oklch $l $c $h), fg: (fg-pick $l $c $h)}
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
    {bg: (oklch $l $c $h), fg: (fg-pick $l $c $h)}
  }
}

# Spectral cascade: each tile is one octave of light frequency, walking the
# EM spectrum from low-energy red (~1.8 eV) through the visible band into UV
# and X-ray territory. Hue and chroma trace the visible spectrum; lightness
# peaks near green (where photopic sensitivity is max) and fades to near-black
# at the top (UV is off-spectrum -- light too energetic for humans to see).
# Hand-tabulated stops park at spectral landmarks rather than fitting a curve.
def spectral-cascade []: nothing -> list {
  let stops = [
    [0.50 0.18  30.0]   # 2:    deep red          (~656nm, H-alpha)
    [0.62 0.19  40.0]   # 4:    orange-red
    [0.70 0.18  55.0]   # 8:    orange
    [0.78 0.18  85.0]   # 16:   yellow            (~580nm, sodium D)
    [0.85 0.17 110.0]   # 32:   yellow-green
    [0.82 0.20 140.0]   # 64:   green             (peak eye sensitivity)
    [0.75 0.16 200.0]   # 128:  cyan              (~486nm, H-beta)
    [0.62 0.20 245.0]   # 256:  blue
    [0.50 0.21 280.0]   # 512:  indigo            (~434nm, H-gamma)
    [0.42 0.20 305.0]   # 1024: violet            (~410nm, H-delta)
    [0.32 0.18 320.0]   # 2048: deep violet       (visible edge)
    [0.22 0.12 320.0]   # 4096: near-UV           (fading from sight)
    [0.10 0.05 320.0]   # 8192: deep UV           (off-spectrum void)
  ]
  $stops | each {|s|
    let l = $s | get 0
    let c = $s | get 1
    let h = $s | get 2
    {bg: (oklch $l $c $h), fg: (fg-pick $l $c $h)}
  }
}

# Sigmoid in lightness: flat plateau, plunge, flat dark plateau on top.
def sigmoid-light []: nothing -> list {
  0..12 | each {|i|
    let t = $i / 12
    let x = 8 * ($t - 0.5)
    let sig = 1 / (1 + (2.718281828459045 ** (0 - $x)))
    let l = 0.93 - 0.55 * $sig
    {bg: (oklch $l 0.13 65), fg: (fg-pick $l 0.13 65)}
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
    (row "Spectral cascade" (spectral-cascade)
      "Each tile is one octave of light frequency. The ladder walks the EM spectrum from H-alpha red (~1.8eV photons) through the visible band to UV. Lightness peaks at green (photopic sensitivity max); top two tiles fade past violet into near-black -- light too energetic to see. Tension builds because higher-energy photons culturally and physically read as more intense.")
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
<p class='lede'>Six color-theory progressions for the same value ladder. Numbers double; what does the color do?</p>
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
