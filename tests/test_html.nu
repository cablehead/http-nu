#!/usr/bin/env nu

use std/assert
use ../src/stdlib/html/mod.nu *

# Test attrs-to-string
assert equal ({class: "foo"} | attrs-to-string) r#' class="foo"'#
assert equal ({class: "foo" id: "bar"} | attrs-to-string) r#' class="foo" id="bar"'#
assert equal ({} | attrs-to-string) ''

# Test div with text content
assert equal (h-div "Hello") '<div>Hello</div>'

# Test div with attrs
assert equal (h-div {class: "card"}) r#'<div class="card"></div>'#

# Test div with attrs + text
assert equal (h-div {class: "card"} "Content") r#'<div class="card">Content</div>'#

# Test media card with siblings
let card = h-div {class: "card"} {
  h-div {class: "title"} "Sunset"
  | h-div {class: "author"} "Photo by Alice"
  | h-div {class: "date"} "2025-12-15"
}
assert equal $card r#'<div class="card"><div class="title">Sunset</div><div class="author">Photo by Alice</div><div class="date">2025-12-15</div></div>'#

# Test media card with img and p
let card = h-div {class: "card"} {
  h-img {src: "sunset.jpg" alt: "Sunset"}
  | h-p "A beautiful sunset over the ocean"
  | h-p {class: "meta"} "Photo by Alice"
}
assert equal $card r#'<div class="card"><img src="sunset.jpg" alt="Sunset"><p>A beautiful sunset over the ocean</p><p class="meta">Photo by Alice</p></div>'#

# Test void tags with no args
assert equal (h-br) '<br>'

# Test void tags with attrs
assert equal (h-hr {class: "divider"}) r#'<hr class="divider">'#

# Test regular tags with no args
assert equal (h-div) '<div></div>'
