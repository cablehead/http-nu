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
assert equal (INPUT {type: "checkbox" checked: true}).__html '<input type="checkbox" checked>'
assert equal (INPUT {type: "checkbox" checked: false}).__html '<input type="checkbox">'

# Test class as list
assert equal ({class: [foo bar baz]} | attrs-to-string) r#' class="foo bar baz"'#
assert equal (DIV {class: [card active]} "x").__html r#'<div class="card active">x</div>'#

# Test style as record
assert equal ({style: {color: red padding: 10px}} | attrs-to-string) r#' style="color: red; padding: 10px;"'#
assert equal (DIV {style: {color: red}} "x").__html r#'<div style="color: red;">x</div>'#

# Test style value as list (comma-separated, e.g. font-family)
assert equal ({style: {font-family: [Arial sans-serif]}} | attrs-to-string) r#' style="font-family: Arial, sans-serif;"'#

# Test div with text content
assert equal (DIV "Hello").__html '<div>Hello</div>'

# Test HTML escaping
assert equal (DIV "<script>alert(1)</script>").__html '<div>&lt;script&gt;alert(1)&lt;/script&gt;</div>'
assert equal (DIV "a < b & c > d").__html '<div>a &lt; b &amp; c &gt; d</div>'
assert equal (DIV {class: "x"} "1 < 2").__html '<div class="x">1 &lt; 2</div>'

# Nested tags should NOT double-escape
assert equal (DIV (SPAN "hi")).__html '<div><span>hi</span></div>'
assert equal (DIV (SPAN "<b>bold</b>")).__html '<div><span>&lt;b&gt;bold&lt;/b&gt;</span></div>'

# Test div with attrs
assert equal (DIV {class: "card"}).__html r#'<div class="card"></div>'#

# Test div with attrs + text
assert equal (DIV {class: "card"} "Content").__html r#'<div class="card">Content</div>'#

# Test void tags with no args
assert equal (BR).__html '<br>'

# Test void tags with attrs
assert equal (HR {class: "divider"}).__html r#'<hr class="divider">'#

# Test regular tags with no args
assert equal (DIV).__html '<div></div>'

# Test siblings with append
assert equal (
  DIV {class: "card"} {
    DIV {class: "title"} "Sunset"
    | append (DIV {class: "author"} "Photo by Alice")
    | append (DIV {class: "date"} "2025-12-15")
  }
).__html (
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
  DIV {class: "card"} {
    IMG {src: "sunset.jpg" alt: "Sunset"}
    | append (P "A beautiful sunset over the ocean")
    | append (P {class: "meta"} "Photo by Alice")
  }
).__html (
  r#'
  <div class="card">
    <img src="sunset.jpg" alt="Sunset">
    <p>A beautiful sunset over the ocean</p>
    <p class="meta">Photo by Alice</p>
  </div>
'# | squish
)

# Test list from each
assert equal (UL { 1..3 | each {|n| LI $"# ($n)" } }).__html (
  r#'
  <ul>
    <li># 1</li>
    <li># 2</li>
    <li># 3</li>
  </ul>
'# | squish
)

# Test single child (no siblings)
assert equal (UL { LI "only" }).__html '<ul><li>only</li></ul>'

# Test nested structure
assert equal (
  DIV {class: "outer"} {
    DIV {class: "inner"} {
      SPAN "nested"
    }
  }
).__html (
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
  UL {
    LI "first"
    | append (1..3 | each {|n| LI $"item ($n)" })
    | append (LI "last")
  }
).__html (
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
  DIV [
    [(H1 "Title") (P "Subtitle")]
    (UL [(LI "a") (LI "b")])
  ]
).__html (
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
assert equal (DIV "a" "b" "c").__html '<div>abc</div>'
assert equal (DIV {class: x} "a" "b" "c").__html '<div class="x">abc</div>'
assert equal (DIV {class: x} (P "a") (P "b")).__html '<div class="x"><p>a</p><p>b</p></div>'
assert equal (DIV (P "a") (P "b")).__html '<div><p>a</p><p>b</p></div>'
assert equal (DIV {class: x} "text" (P "child") "more").__html '<div class="x">text<p>child</p>more</div>'
assert equal (DIV "text" (P "child") "more").__html '<div>text<p>child</p>more</div>'
assert equal (DIV {class: x} [(LI "a") (LI "b")] {|| P "c" | append (P "d") }).__html '<div class="x"><li>a</li><li>b</li><p>c</p><p>d</p></div>'
assert equal (
  SECTION {id: main}
  (H1 "Title")
  [(P "intro") (P "more")]
  {|| UL { LI "x" | append (LI "y") } }
).__html '<section id="main"><h1>Title</h1><p>intro</p><p>more</p><ul><li>x</li><li>y</li></ul></section>'

# Test Jinja2 _var (variable expression)
assert equal (_var "name").__html '{{ name }}'
assert equal (_var "user.email").__html '{{ user.email }}'
assert equal (DIV (_var "content")).__html '<div>{{ content }}</div>'

# Test Jinja2 _for
assert equal (
  _for {item: items} (LI (_var "item.name"))
).__html '{% for item in items %}<li>{{ item.name }}</li>{% endfor %}'

assert equal (
  UL (_for {user: users} (LI (_var "user.name")))
).__html '<ul>{% for user in users %}<li>{{ user.name }}</li>{% endfor %}</ul>'

assert equal (
  DIV {class: "list"}
  (
    _for {item: items}
    (DIV {class: "item"} (_var "item"))
  )
).__html '<div class="list">{% for item in items %}<div class="item">{{ item }}</div>{% endfor %}</div>'

# Test Jinja2 _if
assert equal (
  _if "user.admin" (DIV "Admin Panel")
).__html '{% if user.admin %}<div>Admin Panel</div>{% endif %}'

assert equal (
  DIV (_if "show" (LI "visible"))
).__html '<div>{% if show %}<li>visible</li>{% endif %}</div>'

assert equal (
  _if "items"
  (UL (_for {item: items} (LI (_var "item"))))
).__html '{% if items %}<ul>{% for item in items %}<li>{{ item }}</li>{% endfor %}</ul>{% endif %}'

# Test escaping in _for/_if (raw strings are escaped)
assert equal (
  _for {x: xs} (LI "<script>bad</script>")
).__html '{% for x in xs %}<li>&lt;script&gt;bad&lt;/script&gt;</li>{% endfor %}'
