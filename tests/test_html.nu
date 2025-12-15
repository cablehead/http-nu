#!/usr/bin/env nu

use std/assert
use ../src/stdlib/html/mod.nu *

# Test div with text content
let result = h-div "Hello"
assert equal $result "<div>Hello</div>"

# Test div with attrs
let result = h-div {class: "card"}
assert equal $result "<div class=\"card\"></div>"

# Test div with attrs + text
let result = h-div {class: "card"} "Content"
assert equal $result "<div class=\"card\">Content</div>"

# Test media card with siblings
let card = h-div {class: "card"} {
  h-div {class: "title"} "Sunset"
  | h-div {class: "author"} "Photo by Alice"
  | h-div {class: "date"} "2025-12-15"
}
assert equal $card "<div class=\"card\"><div class=\"title\">Sunset</div><div class=\"author\">Photo by Alice</div><div class=\"date\">2025-12-15</div></div>"

# Test media card with img and p
let card = h-div {class: "card"} {
  h-img {src: "sunset.jpg" alt: "Sunset"}
  | h-p "A beautiful sunset over the ocean"
  | h-p {class: "meta"} "Photo by Alice"
}
assert equal $card "<div class=\"card\"><img src=\"sunset.jpg\" alt=\"Sunset\"><p>A beautiful sunset over the ocean</p><p class=\"meta\">Photo by Alice</p></div>"

# Test void tags with no args
let result = h-br
assert equal $result "<br>"

# Test void tags with attrs
let result = h-hr {class: "divider"}
assert equal $result "<hr class=\"divider\">"

# Test regular tags with no args
let result = h-div
assert equal $result "<div></div>"
