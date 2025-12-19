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

# Test boolean attributes
assert equal ({disabled: true} | attrs-to-string) ' disabled'
assert equal ({disabled: false} | attrs-to-string) ''
assert equal ({class: "btn" disabled: true} | attrs-to-string) r#' class="btn" disabled'#
assert equal ({class: "btn" disabled: false} | attrs-to-string) r#' class="btn"'#
assert equal (_input {type: "checkbox" checked: true}) '<input type="checkbox" checked>'
assert equal (_input {type: "checkbox" checked: false}) '<input type="checkbox">'

# Test class as list
assert equal ({class: [foo bar baz]} | attrs-to-string) r#' class="foo bar baz"'#
assert equal (_div {class: [card active]} "x") r#'<div class="card active">x</div>'#

# Test style as record
assert equal ({style: {color: red padding: 10px}} | attrs-to-string) r#' style="color: red; padding: 10px;"'#
assert equal (_div {style: {color: red}} "x") r#'<div style="color: red;">x</div>'#

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
assert equal (
  _div {class: "card"} {
    _div {class: "title"} "Sunset"
    | append (_div {class: "author"} "Photo by Alice")
    | append (_div {class: "date"} "2025-12-15")
  }
) (
  r#'
  <div class="card">
    <div class="title">Sunset</div>
    <div class="author">Photo by Alice</div>
    <div class="date">2025-12-15</div>
  </div>
'# | squish
)

# Test mixed void and regular tags with append
assert equal (
  _div {class: "card"} {
    _img {src: "sunset.jpg" alt: "Sunset"}
    | append (_p "A beautiful sunset over the ocean")
    | append (_p {class: "meta"} "Photo by Alice")
  }
) (
  r#'
  <div class="card">
    <img src="sunset.jpg" alt="Sunset">
    <p>A beautiful sunset over the ocean</p>
    <p class="meta">Photo by Alice</p>
  </div>
'# | squish
)

# Test list from each
assert equal (_ul { 1..3 | each {|n| _li $"# ($n)" } }) (
  r#'
  <ul>
    <li># 1</li>
    <li># 2</li>
    <li># 3</li>
  </ul>
'# | squish
)

# Test single child (no siblings)
assert equal (_ul { _li "only" }) '<ul><li>only</li></ul>'

# Test nested structure
assert equal (
  _div {class: "outer"} {
    _div {class: "inner"} {
      _span "nested"
    }
  }
) (
  r#'
  <div class="outer">
    <div class="inner">
      <span>nested</span>
    </div>
  </div>
'# | squish
)

# Test each with append for mixed content
assert equal (
  _ul {
    _li "first"
    | append (1..3 | each {|n| _li $"item ($n)" })
    | append (_li "last")
  }
) (
  r#'
  <ul>
    <li>first</li>
    <li>item 1</li>
    <li>item 2</li>
    <li>item 3</li>
    <li>last</li>
  </ul>
'# | squish
)

# Test nested list children (recursive to-children)
assert equal (
  _div [
    [(_h1 "Title") (_p "Subtitle")]
    (_ul [(_li "a") (_li "b")])
  ]
) (
  r#'
  <div>
    <h1>Title</h1>
    <p>Subtitle</p>
    <ul>
      <li>a</li>
      <li>b</li>
    </ul>
  </div>
'# | squish
)

# Test variadic args permutations
assert equal (_div "a" "b" "c") '<div>abc</div>'
assert equal (_div {class: x} "a" "b" "c") '<div class="x">abc</div>'
assert equal (_div {class: x} (_p "a") (_p "b")) '<div class="x"><p>a</p><p>b</p></div>'
assert equal (_div (_p "a") (_p "b")) '<div><p>a</p><p>b</p></div>'
assert equal (_div {class: x} "text" (_p "child") "more") '<div class="x">text<p>child</p>more</div>'
assert equal (_div "text" (_p "child") "more") '<div>text<p>child</p>more</div>'
assert equal (_div {class: x} [(_li "a") (_li "b")] {|| _p "c" | +p "d"}) '<div class="x"><li>a</li><li>b</li><p>c</p><p>d</p></div>'
assert equal (
  _section {id: main}
    (_h1 "Title")
    [(_p "intro") (_p "more")]
    {|| _ul { _li "x" | +li "y" }}
) '<section id="main"><h1>Title</h1><p>intro</p><p>more</p><ul><li>x</li><li>y</li></ul></section>'
