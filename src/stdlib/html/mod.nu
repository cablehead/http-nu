# HTML DSL for nushell

# Escape HTML special characters
def escape-html []: string -> string {
  $in | str replace -a '&' '&amp;' | str replace -a '<' '&lt;' | str replace -a '>' '&gt;'
}

# Normalize content to string, joining lists
def to-children []: any -> string {
  let input = $in
  let type = $input | describe -d | get type
  match $type {
    'string' => ($input | escape-html)
    'list' => ($input | each { to-children } | str join)
    'closure' => (do $input | to-children)
    'record' => (if '__html' in $input { $input.__html } else { "" })
    _ => ""
  }
}

# {class: "foo"} -> ' class="foo"'
# style can be a record: {style: {color: red, padding: 10px}} -> ' style="color: red; padding: 10px;"'
# boolean: {disabled: true} -> ' disabled', {disabled: false} -> ''
export def attrs-to-string []: record -> string {
  $in
  | transpose key value
  | each {|attr|
    if $attr.value == false { return "" }
    if $attr.value == true { return $attr.key }
    let value = if $attr.key == "style" and ($attr.value | describe -d | get type) == "record" {
      $attr.value | transpose k v | each {|p|
        let v = if ($p.v | describe -d | get type) == "list" { $p.v | str join ", " } else { $p.v }
        $"($p.k): ($v);"
      } | str join " "
    } else if $attr.key == "class" and ($attr.value | describe -d | get type) == "list" {
      $attr.value | str join " "
    } else {
      $attr.value
    }
    $'($attr.key)="($value)"'
  }
  | where $it != ""
  | str join ' '
  | if ($in | is-empty) { "" } else { $" ($in)" }
}

# Render a tag with optional attributes and children
export def render-tag [tag: string ...args: any]: nothing -> record {
  # Normalize to [attrs, ...content] - prepend {} if first arg isn't a record
  let args = if ($args | is-not-empty) and ($args | first | describe -d | get type) == 'record' and '__html' not-in ($args | first) { $args } else { $args | prepend {} }
  let attrs = $args | first
  let content = $args | skip 1
  let children = $content | each { to-children } | str join
  let attrs_str = $attrs | attrs-to-string
  {__html: $"<($tag)($attrs_str)>($children)</($tag)>"}
}

# Render a void tag (no children allowed per HTML spec)
export def render-void-tag [tag: string attrs?: record]: nothing -> record {
  let attrs_str = ($attrs | default {}) | attrs-to-string
  {__html: $"<($tag)($attrs_str)>"}
}

# Jinja2 control flow
export def _for [binding: record, ...body: any]: nothing -> record {
  let var = $binding | columns | first
  let collection = $binding | values | first
  let children = $body | each { to-children } | str join
  {__html: $"{% for ($var) in ($collection) %}($children){% endfor %}"}
}

export def _if [cond: string, ...body: any]: nothing -> record {
  let children = $body | each { to-children } | str join
  {__html: $"{% if ($cond) %}($children){% endif %}"}
}

# Jinja2 variable expression (not escaped)
export def _var [expr: string]: nothing -> record {
  {__html: $"{{ ($expr) }}"}
}

# Document metadata
export def HTML [...args: any]: nothing -> record { render-tag html ...$args }
export def HEAD [...args: any]: nothing -> record { render-tag head ...$args }
export def TITLE [...args: any]: nothing -> record { render-tag title ...$args }
export def BASE [attrs?: record]: nothing -> record { render-void-tag base $attrs }
export def LINK [attrs?: record]: nothing -> record { render-void-tag link $attrs }
export def META [attrs?: record]: nothing -> record { render-void-tag meta $attrs }
export def STYLE [...args: any]: nothing -> record { render-tag style ...$args }

# Sections
export def BODY [...args: any]: nothing -> record { render-tag body ...$args }
export def ARTICLE [...args: any]: nothing -> record { render-tag article ...$args }
export def SECTION [...args: any]: nothing -> record { render-tag section ...$args }
export def NAV [...args: any]: nothing -> record { render-tag nav ...$args }
export def ASIDE [...args: any]: nothing -> record { render-tag aside ...$args }
export def HEADER [...args: any]: nothing -> record { render-tag header ...$args }
export def FOOTER [...args: any]: nothing -> record { render-tag footer ...$args }
export def MAIN [...args: any]: nothing -> record { render-tag main ...$args }

# Grouping content
export def DIV [...args: any]: nothing -> record { render-tag div ...$args }
export def P [...args: any]: nothing -> record { render-tag p ...$args }
export def HR [attrs?: record]: nothing -> record { render-void-tag hr $attrs }
export def PRE [...args: any]: nothing -> record { render-tag pre ...$args }
export def BLOCKQUOTE [...args: any]: nothing -> record { render-tag blockquote ...$args }
export def OL [...args: any]: nothing -> record { render-tag ol ...$args }
export def UL [...args: any]: nothing -> record { render-tag ul ...$args }
export def LI [...args: any]: nothing -> record { render-tag li ...$args }
export def DL [...args: any]: nothing -> record { render-tag dl ...$args }
export def DT [...args: any]: nothing -> record { render-tag dt ...$args }
export def DD [...args: any]: nothing -> record { render-tag dd ...$args }
export def FIGURE [...args: any]: nothing -> record { render-tag figure ...$args }
export def FIGCAPTION [...args: any]: nothing -> record { render-tag figcaption ...$args }

# Text content
export def A [...args: any]: nothing -> record { render-tag a ...$args }
export def EM [...args: any]: nothing -> record { render-tag em ...$args }
export def STRONG [...args: any]: nothing -> record { render-tag strong ...$args }
export def SMALL [...args: any]: nothing -> record { render-tag small ...$args }
export def CODE [...args: any]: nothing -> record { render-tag code ...$args }
export def SPAN [...args: any]: nothing -> record { render-tag span ...$args }
export def BR [attrs?: record]: nothing -> record { render-void-tag br $attrs }
export def WBR [attrs?: record]: nothing -> record { render-void-tag wbr $attrs }

# Embedded content
export def IMG [attrs?: record]: nothing -> record { render-void-tag img $attrs }
export def IFRAME [...args: any]: nothing -> record { render-tag iframe ...$args }
export def EMBED [attrs?: record]: nothing -> record { render-void-tag embed $attrs }
export def VIDEO [...args: any]: nothing -> record { render-tag video ...$args }
export def AUDIO [...args: any]: nothing -> record { render-tag audio ...$args }
export def SOURCE [attrs?: record]: nothing -> record { render-void-tag source $attrs }
export def TRACK [attrs?: record]: nothing -> record { render-void-tag track $attrs }
export def CANVAS [...args: any]: nothing -> record { render-tag canvas ...$args }
export def SVG [...args: any]: nothing -> record { render-tag svg ...$args }

# Tables
export def TABLE [...args: any]: nothing -> record { render-tag table ...$args }
export def CAPTION [...args: any]: nothing -> record { render-tag caption ...$args }
export def THEAD [...args: any]: nothing -> record { render-tag thead ...$args }
export def TBODY [...args: any]: nothing -> record { render-tag tbody ...$args }
export def TFOOT [...args: any]: nothing -> record { render-tag tfoot ...$args }
export def TR [...args: any]: nothing -> record { render-tag tr ...$args }
export def TH [...args: any]: nothing -> record { render-tag th ...$args }
export def TD [...args: any]: nothing -> record { render-tag td ...$args }
export def COL [attrs?: record]: nothing -> record { render-void-tag col $attrs }
export def COLGROUP [...args: any]: nothing -> record { render-tag colgroup ...$args }

# Forms
export def FORM [...args: any]: nothing -> record { render-tag form ...$args }
export def LABEL [...args: any]: nothing -> record { render-tag label ...$args }
export def INPUT [attrs?: record]: nothing -> record { render-void-tag input $attrs }
export def BUTTON [...args: any]: nothing -> record { render-tag button ...$args }
export def SELECT [...args: any]: nothing -> record { render-tag select ...$args }
export def OPTION [...args: any]: nothing -> record { render-tag option ...$args }
export def TEXTAREA [...args: any]: nothing -> record { render-tag textarea ...$args }
export def FIELDSET [...args: any]: nothing -> record { render-tag fieldset ...$args }
export def LEGEND [...args: any]: nothing -> record { render-tag legend ...$args }

# Headings
export def H1 [...args: any]: nothing -> record { render-tag h1 ...$args }
export def H2 [...args: any]: nothing -> record { render-tag h2 ...$args }
export def H3 [...args: any]: nothing -> record { render-tag h3 ...$args }
export def H4 [...args: any]: nothing -> record { render-tag h4 ...$args }
export def H5 [...args: any]: nothing -> record { render-tag h5 ...$args }
export def H6 [...args: any]: nothing -> record { render-tag h6 ...$args }

# Scripting
export def SCRIPT [...args: any]: nothing -> record { render-tag script ...$args }
export def NOSCRIPT [...args: any]: nothing -> record { render-tag noscript ...$args }
export def TEMPLATE [...args: any]: nothing -> record { render-tag template ...$args }
export def SLOT [...args: any]: nothing -> record { render-tag slot ...$args }

# Additional inline text
export def ABBR [...args: any]: nothing -> record { render-tag abbr ...$args }
export def B [...args: any]: nothing -> record { render-tag b ...$args }
export def I [...args: any]: nothing -> record { render-tag i ...$args }
export def U [...args: any]: nothing -> record { render-tag u ...$args }
export def S [...args: any]: nothing -> record { render-tag s ...$args }
export def MARK [...args: any]: nothing -> record { render-tag mark ...$args }
export def Q [...args: any]: nothing -> record { render-tag q ...$args }
export def CITE [...args: any]: nothing -> record { render-tag cite ...$args }
export def DFN [...args: any]: nothing -> record { render-tag dfn ...$args }
export def KBD [...args: any]: nothing -> record { render-tag kbd ...$args }
export def SAMP [...args: any]: nothing -> record { render-tag samp ...$args }
export def VAR [...args: any]: nothing -> record { render-tag var ...$args }
export def SUB [...args: any]: nothing -> record { render-tag sub ...$args }
export def SUP [...args: any]: nothing -> record { render-tag sup ...$args }
export def TIME [...args: any]: nothing -> record { render-tag time ...$args }
export def DATA [...args: any]: nothing -> record { render-tag data ...$args }

# Edits
export def DEL [...args: any]: nothing -> record { render-tag del ...$args }
export def INS [...args: any]: nothing -> record { render-tag ins ...$args }

# Interactive
export def DETAILS [...args: any]: nothing -> record { render-tag details ...$args }
export def SUMMARY [...args: any]: nothing -> record { render-tag summary ...$args }
export def DIALOG [...args: any]: nothing -> record { render-tag dialog ...$args }

# Additional content
export def ADDRESS [...args: any]: nothing -> record { render-tag address ...$args }
export def HGROUP [...args: any]: nothing -> record { render-tag hgroup ...$args }
export def SEARCH [...args: any]: nothing -> record { render-tag search ...$args }
export def MENU [...args: any]: nothing -> record { render-tag menu ...$args }

# Image maps
export def AREA [attrs?: record]: nothing -> record { render-void-tag area $attrs }
export def MAP [...args: any]: nothing -> record { render-tag map ...$args }
export def PICTURE [...args: any]: nothing -> record { render-tag picture ...$args }

# Form additions
export def METER [...args: any]: nothing -> record { render-tag meter ...$args }
export def PROGRESS [...args: any]: nothing -> record { render-tag progress ...$args }
export def OUTPUT [...args: any]: nothing -> record { render-tag output ...$args }
export def DATALIST [...args: any]: nothing -> record { render-tag datalist ...$args }
export def OPTGROUP [...args: any]: nothing -> record { render-tag optgroup ...$args }

# Ruby annotations
export def RUBY [...args: any]: nothing -> record { render-tag ruby ...$args }
export def RT [...args: any]: nothing -> record { render-tag rt ...$args }
export def RP [...args: any]: nothing -> record { render-tag rp ...$args }

# Bidirectional text
export def BDI [...args: any]: nothing -> record { render-tag bdi ...$args }
export def BDO [...args: any]: nothing -> record { render-tag bdo ...$args }
