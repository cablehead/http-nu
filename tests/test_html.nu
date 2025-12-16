#!/usr/bin/env nu

use std/assert
use ../src/stdlib/html/mod.nu *

# Collapse formatted HTML to single line for comparison
def squish []: string -> string {
  $in | str replace -ra '\n\s*' ''
}

# Test attrs-to-string
assert equal ({class: "foo"} | attrs-to-string) r#' class="foo"'#
assert equal ({class: "foo" id: "bar"} | attrs-to-string) r#' class="foo" id="bar"'#
assert equal ({} | attrs-to-string) ''

# Test div with text content
assert equal (_div "Hello") '<div>Hello</div>'

# Test div with attrs
assert equal (_div {class: "card"}) r#'<div class="card"></div>'#

# Test div with attrs + text
assert equal (_div {class: "card"} "Content") r#'<div class="card">Content</div>'#

# Test void tags with no args
assert equal (_br) '<br>'

# Test void tags with attrs
assert equal (_hr {class: "divider"}) r#'<hr class="divider">'#

# Test regular tags with no args
assert equal (_div) '<div></div>'

# Test siblings with append
assert equal (_div {class: "card"} {
  _div {class: "title"} "Sunset"
  | append (_div {class: "author"} "Photo by Alice")
  | append (_div {class: "date"} "2025-12-15")
}) (r#'
  <div class="card">
    <div class="title">Sunset</div>
    <div class="author">Photo by Alice</div>
    <div class="date">2025-12-15</div>
  </div>
'# | squish)

# Test mixed void and regular tags with append
assert equal (_div {class: "card"} {
  _img {src: "sunset.jpg" alt: "Sunset"}
  | append (_p "A beautiful sunset over the ocean")
  | append (_p {class: "meta"} "Photo by Alice")
}) (r#'
  <div class="card">
    <img src="sunset.jpg" alt="Sunset">
    <p>A beautiful sunset over the ocean</p>
    <p class="meta">Photo by Alice</p>
  </div>
'# | squish)

# Test list from each
assert equal (_ul { 1..3 | each {|n| _li $"# ($n)" } }) (r#'
  <ul>
    <li># 1</li>
    <li># 2</li>
    <li># 3</li>
  </ul>
'# | squish)

# Test single child (no siblings)
assert equal (_ul { _li "only" }) '<ul><li>only</li></ul>'

# Test nested structure
assert equal (_div {class: "outer"} {
  _div {class: "inner"} {
    _span "nested"
  }
}) (r#'
  <div class="outer">
    <div class="inner">
      <span>nested</span>
    </div>
  </div>
'# | squish)

# Test each with append for mixed content
assert equal (_ul {
  _li "first"
  | append (1..3 | each {|n| _li $"item ($n)" })
  | append (_li "last")
}) (r#'
  <ul>
    <li>first</li>
    <li>item 1</li>
    <li>item 2</li>
    <li>item 3</li>
    <li>last</li>
  </ul>
'# | squish)
