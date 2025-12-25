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

# Document metadata
export def _html [...args: any]: nothing -> record { render-tag html ...$args }
export def _head [...args: any]: nothing -> record { render-tag head ...$args }
export def _title [...args: any]: nothing -> record { render-tag title ...$args }
export def _base [attrs?: record]: nothing -> record { render-void-tag base $attrs }
export def _link [attrs?: record]: nothing -> record { render-void-tag link $attrs }
export def _meta [attrs?: record]: nothing -> record { render-void-tag meta $attrs }
export def _style [...args: any]: nothing -> record { render-tag style ...$args }

# Sections
export def _body [...args: any]: nothing -> record { render-tag body ...$args }
export def _article [...args: any]: nothing -> record { render-tag article ...$args }
export def _section [...args: any]: nothing -> record { render-tag section ...$args }
export def _nav [...args: any]: nothing -> record { render-tag nav ...$args }
export def _aside [...args: any]: nothing -> record { render-tag aside ...$args }
export def _header [...args: any]: nothing -> record { render-tag header ...$args }
export def _footer [...args: any]: nothing -> record { render-tag footer ...$args }
export def _main [...args: any]: nothing -> record { render-tag main ...$args }

# Grouping content
export def _div [...args: any]: nothing -> record { render-tag div ...$args }
export def _p [...args: any]: nothing -> record { render-tag p ...$args }
export def _hr [attrs?: record]: nothing -> record { render-void-tag hr $attrs }
export def _pre [...args: any]: nothing -> record { render-tag pre ...$args }
export def _blockquote [...args: any]: nothing -> record { render-tag blockquote ...$args }
export def _ol [...args: any]: nothing -> record { render-tag ol ...$args }
export def _ul [...args: any]: nothing -> record { render-tag ul ...$args }
export def _li [...args: any]: nothing -> record { render-tag li ...$args }
export def _dl [...args: any]: nothing -> record { render-tag dl ...$args }
export def _dt [...args: any]: nothing -> record { render-tag dt ...$args }
export def _dd [...args: any]: nothing -> record { render-tag dd ...$args }
export def _figure [...args: any]: nothing -> record { render-tag figure ...$args }
export def _figcaption [...args: any]: nothing -> record { render-tag figcaption ...$args }

# Text content
export def _a [...args: any]: nothing -> record { render-tag a ...$args }
export def _em [...args: any]: nothing -> record { render-tag em ...$args }
export def _strong [...args: any]: nothing -> record { render-tag strong ...$args }
export def _small [...args: any]: nothing -> record { render-tag small ...$args }
export def _code [...args: any]: nothing -> record { render-tag code ...$args }
export def _span [...args: any]: nothing -> record { render-tag span ...$args }
export def _br [attrs?: record]: nothing -> record { render-void-tag br $attrs }
export def _wbr [attrs?: record]: nothing -> record { render-void-tag wbr $attrs }

# Embedded content
export def _img [attrs?: record]: nothing -> record { render-void-tag img $attrs }
export def _iframe [...args: any]: nothing -> record { render-tag iframe ...$args }
export def _embed [attrs?: record]: nothing -> record { render-void-tag embed $attrs }
export def _video [...args: any]: nothing -> record { render-tag video ...$args }
export def _audio [...args: any]: nothing -> record { render-tag audio ...$args }
export def _source [attrs?: record]: nothing -> record { render-void-tag source $attrs }
export def _track [attrs?: record]: nothing -> record { render-void-tag track $attrs }
export def _canvas [...args: any]: nothing -> record { render-tag canvas ...$args }
export def _svg [...args: any]: nothing -> record { render-tag svg ...$args }

# Tables
export def _table [...args: any]: nothing -> record { render-tag table ...$args }
export def _caption [...args: any]: nothing -> record { render-tag caption ...$args }
export def _thead [...args: any]: nothing -> record { render-tag thead ...$args }
export def _tbody [...args: any]: nothing -> record { render-tag tbody ...$args }
export def _tfoot [...args: any]: nothing -> record { render-tag tfoot ...$args }
export def _tr [...args: any]: nothing -> record { render-tag tr ...$args }
export def _th [...args: any]: nothing -> record { render-tag th ...$args }
export def _td [...args: any]: nothing -> record { render-tag td ...$args }
export def _col [attrs?: record]: nothing -> record { render-void-tag col $attrs }
export def _colgroup [...args: any]: nothing -> record { render-tag colgroup ...$args }

# Forms
export def _form [...args: any]: nothing -> record { render-tag form ...$args }
export def _label [...args: any]: nothing -> record { render-tag label ...$args }
export def _input [attrs?: record]: nothing -> record { render-void-tag input $attrs }
export def _button [...args: any]: nothing -> record { render-tag button ...$args }
export def _select [...args: any]: nothing -> record { render-tag select ...$args }
export def _option [...args: any]: nothing -> record { render-tag option ...$args }
export def _textarea [...args: any]: nothing -> record { render-tag textarea ...$args }
export def _fieldset [...args: any]: nothing -> record { render-tag fieldset ...$args }
export def _legend [...args: any]: nothing -> record { render-tag legend ...$args }

# Headings
export def _h1 [...args: any]: nothing -> record { render-tag h1 ...$args }
export def _h2 [...args: any]: nothing -> record { render-tag h2 ...$args }
export def _h3 [...args: any]: nothing -> record { render-tag h3 ...$args }
export def _h4 [...args: any]: nothing -> record { render-tag h4 ...$args }
export def _h5 [...args: any]: nothing -> record { render-tag h5 ...$args }
export def _h6 [...args: any]: nothing -> record { render-tag h6 ...$args }

# Scripting
export def _script [...args: any]: nothing -> record { render-tag script ...$args }
export def _noscript [...args: any]: nothing -> record { render-tag noscript ...$args }
export def _template [...args: any]: nothing -> record { render-tag template ...$args }
export def _slot [...args: any]: nothing -> record { render-tag slot ...$args }

# Additional inline text
export def _abbr [...args: any]: nothing -> record { render-tag abbr ...$args }
export def _b [...args: any]: nothing -> record { render-tag b ...$args }
export def _i [...args: any]: nothing -> record { render-tag i ...$args }
export def _u [...args: any]: nothing -> record { render-tag u ...$args }
export def _s [...args: any]: nothing -> record { render-tag s ...$args }
export def _mark [...args: any]: nothing -> record { render-tag mark ...$args }
export def _q [...args: any]: nothing -> record { render-tag q ...$args }
export def _cite [...args: any]: nothing -> record { render-tag cite ...$args }
export def _dfn [...args: any]: nothing -> record { render-tag dfn ...$args }
export def _kbd [...args: any]: nothing -> record { render-tag kbd ...$args }
export def _samp [...args: any]: nothing -> record { render-tag samp ...$args }
export def _var [...args: any]: nothing -> record { render-tag var ...$args }
export def _sub [...args: any]: nothing -> record { render-tag sub ...$args }
export def _sup [...args: any]: nothing -> record { render-tag sup ...$args }
export def _time [...args: any]: nothing -> record { render-tag time ...$args }
export def _data [...args: any]: nothing -> record { render-tag data ...$args }

# Edits
export def _del [...args: any]: nothing -> record { render-tag del ...$args }
export def _ins [...args: any]: nothing -> record { render-tag ins ...$args }

# Interactive
export def _details [...args: any]: nothing -> record { render-tag details ...$args }
export def _summary [...args: any]: nothing -> record { render-tag summary ...$args }
export def _dialog [...args: any]: nothing -> record { render-tag dialog ...$args }

# Additional content
export def _address [...args: any]: nothing -> record { render-tag address ...$args }
export def _hgroup [...args: any]: nothing -> record { render-tag hgroup ...$args }
export def _search [...args: any]: nothing -> record { render-tag search ...$args }
export def _menu [...args: any]: nothing -> record { render-tag menu ...$args }

# Image maps
export def _area [attrs?: record]: nothing -> record { render-void-tag area $attrs }
export def _map [...args: any]: nothing -> record { render-tag map ...$args }
export def _picture [...args: any]: nothing -> record { render-tag picture ...$args }

# Form additions
export def _meter [...args: any]: nothing -> record { render-tag meter ...$args }
export def _progress [...args: any]: nothing -> record { render-tag progress ...$args }
export def _output [...args: any]: nothing -> record { render-tag output ...$args }
export def _datalist [...args: any]: nothing -> record { render-tag datalist ...$args }
export def _optgroup [...args: any]: nothing -> record { render-tag optgroup ...$args }

# Ruby annotations
export def _ruby [...args: any]: nothing -> record { render-tag ruby ...$args }
export def _rt [...args: any]: nothing -> record { render-tag rt ...$args }
export def _rp [...args: any]: nothing -> record { render-tag rp ...$args }

# Bidirectional text
export def _bdi [...args: any]: nothing -> record { render-tag bdi ...$args }
export def _bdo [...args: any]: nothing -> record { render-tag bdo ...$args }

# Jinja2 control flow
export def _for [binding: list, ...body: any]: nothing -> record {
  let var = $binding | first
  let collection = $binding | last
  let children = $body | each { to-children } | str join
  {__html: $"{% for ($var) in ($collection) %}($children){% endfor %}"}
}

export def _if [cond: string, ...body: any]: nothing -> record {
  let children = $body | each { to-children } | str join
  {__html: $"{% if ($cond) %}($children){% endif %}"}
}

# Jinja2 variable expression (not escaped)
export def _j [expr: string]: nothing -> record {
  {__html: $"{{ ($expr) }}"}
}

# Append variants (+tag) - pipe-friendly siblings: _div "x" | +p "y" => [<div>x</div> <p>y</p>]
export def +a [...args: any]: any -> list { append (_a ...$args) }
export def +abbr [...args: any]: any -> list { append (_abbr ...$args) }
export def +address [...args: any]: any -> list { append (_address ...$args) }
export def +area [attrs?: record]: any -> list { append (_area $attrs) }
export def +article [...args: any]: any -> list { append (_article ...$args) }
export def +aside [...args: any]: any -> list { append (_aside ...$args) }
export def +audio [...args: any]: any -> list { append (_audio ...$args) }
export def +b [...args: any]: any -> list { append (_b ...$args) }
export def +base [attrs?: record]: any -> list { append (_base $attrs) }
export def +bdi [...args: any]: any -> list { append (_bdi ...$args) }
export def +bdo [...args: any]: any -> list { append (_bdo ...$args) }
export def +blockquote [...args: any]: any -> list { append (_blockquote ...$args) }
export def +body [...args: any]: any -> list { append (_body ...$args) }
export def +br [attrs?: record]: any -> list { append (_br $attrs) }
export def +button [...args: any]: any -> list { append (_button ...$args) }
export def +canvas [...args: any]: any -> list { append (_canvas ...$args) }
export def +caption [...args: any]: any -> list { append (_caption ...$args) }
export def +cite [...args: any]: any -> list { append (_cite ...$args) }
export def +code [...args: any]: any -> list { append (_code ...$args) }
export def +col [attrs?: record]: any -> list { append (_col $attrs) }
export def +colgroup [...args: any]: any -> list { append (_colgroup ...$args) }
export def +data [...args: any]: any -> list { append (_data ...$args) }
export def +datalist [...args: any]: any -> list { append (_datalist ...$args) }
export def +dd [...args: any]: any -> list { append (_dd ...$args) }
export def +del [...args: any]: any -> list { append (_del ...$args) }
export def +details [...args: any]: any -> list { append (_details ...$args) }
export def +dfn [...args: any]: any -> list { append (_dfn ...$args) }
export def +dialog [...args: any]: any -> list { append (_dialog ...$args) }
export def +div [...args: any]: any -> list { append (_div ...$args) }
export def +dl [...args: any]: any -> list { append (_dl ...$args) }
export def +dt [...args: any]: any -> list { append (_dt ...$args) }
export def +em [...args: any]: any -> list { append (_em ...$args) }
export def +embed [attrs?: record]: any -> list { append (_embed $attrs) }
export def +fieldset [...args: any]: any -> list { append (_fieldset ...$args) }
export def +figcaption [...args: any]: any -> list { append (_figcaption ...$args) }
export def +figure [...args: any]: any -> list { append (_figure ...$args) }
export def +footer [...args: any]: any -> list { append (_footer ...$args) }
export def +form [...args: any]: any -> list { append (_form ...$args) }
export def +h1 [...args: any]: any -> list { append (_h1 ...$args) }
export def +h2 [...args: any]: any -> list { append (_h2 ...$args) }
export def +h3 [...args: any]: any -> list { append (_h3 ...$args) }
export def +h4 [...args: any]: any -> list { append (_h4 ...$args) }
export def +h5 [...args: any]: any -> list { append (_h5 ...$args) }
export def +h6 [...args: any]: any -> list { append (_h6 ...$args) }
export def +head [...args: any]: any -> list { append (_head ...$args) }
export def +header [...args: any]: any -> list { append (_header ...$args) }
export def +hgroup [...args: any]: any -> list { append (_hgroup ...$args) }
export def +hr [attrs?: record]: any -> list { append (_hr $attrs) }
export def +html [...args: any]: any -> list { append (_html ...$args) }
export def +i [...args: any]: any -> list { append (_i ...$args) }
export def +iframe [...args: any]: any -> list { append (_iframe ...$args) }
export def +img [attrs?: record]: any -> list { append (_img $attrs) }
export def +input [attrs?: record]: any -> list { append (_input $attrs) }
export def +ins [...args: any]: any -> list { append (_ins ...$args) }
export def +kbd [...args: any]: any -> list { append (_kbd ...$args) }
export def +label [...args: any]: any -> list { append (_label ...$args) }
export def +legend [...args: any]: any -> list { append (_legend ...$args) }
export def +li [...args: any]: any -> list { append (_li ...$args) }
export def +link [attrs?: record]: any -> list { append (_link $attrs) }
export def +main [...args: any]: any -> list { append (_main ...$args) }
export def +map [...args: any]: any -> list { append (_map ...$args) }
export def +mark [...args: any]: any -> list { append (_mark ...$args) }
export def +menu [...args: any]: any -> list { append (_menu ...$args) }
export def +meta [attrs?: record]: any -> list { append (_meta $attrs) }
export def +meter [...args: any]: any -> list { append (_meter ...$args) }
export def +nav [...args: any]: any -> list { append (_nav ...$args) }
export def +noscript [...args: any]: any -> list { append (_noscript ...$args) }
export def +ol [...args: any]: any -> list { append (_ol ...$args) }
export def +optgroup [...args: any]: any -> list { append (_optgroup ...$args) }
export def +option [...args: any]: any -> list { append (_option ...$args) }
export def +output [...args: any]: any -> list { append (_output ...$args) }
export def +p [...args: any]: any -> list { append (_p ...$args) }
export def +picture [...args: any]: any -> list { append (_picture ...$args) }
export def +pre [...args: any]: any -> list { append (_pre ...$args) }
export def +progress [...args: any]: any -> list { append (_progress ...$args) }
export def +q [...args: any]: any -> list { append (_q ...$args) }
export def +rp [...args: any]: any -> list { append (_rp ...$args) }
export def +rt [...args: any]: any -> list { append (_rt ...$args) }
export def +ruby [...args: any]: any -> list { append (_ruby ...$args) }
export def +s [...args: any]: any -> list { append (_s ...$args) }
export def +samp [...args: any]: any -> list { append (_samp ...$args) }
export def +script [...args: any]: any -> list { append (_script ...$args) }
export def +search [...args: any]: any -> list { append (_search ...$args) }
export def +section [...args: any]: any -> list { append (_section ...$args) }
export def +select [...args: any]: any -> list { append (_select ...$args) }
export def +slot [...args: any]: any -> list { append (_slot ...$args) }
export def +small [...args: any]: any -> list { append (_small ...$args) }
export def +source [attrs?: record]: any -> list { append (_source $attrs) }
export def +span [...args: any]: any -> list { append (_span ...$args) }
export def +strong [...args: any]: any -> list { append (_strong ...$args) }
export def +style [...args: any]: any -> list { append (_style ...$args) }
export def +sub [...args: any]: any -> list { append (_sub ...$args) }
export def +summary [...args: any]: any -> list { append (_summary ...$args) }
export def +sup [...args: any]: any -> list { append (_sup ...$args) }
export def +svg [...args: any]: any -> list { append (_svg ...$args) }
export def +table [...args: any]: any -> list { append (_table ...$args) }
export def +tbody [...args: any]: any -> list { append (_tbody ...$args) }
export def +td [...args: any]: any -> list { append (_td ...$args) }
export def +template [...args: any]: any -> list { append (_template ...$args) }
export def +textarea [...args: any]: any -> list { append (_textarea ...$args) }
export def +tfoot [...args: any]: any -> list { append (_tfoot ...$args) }
export def +th [...args: any]: any -> list { append (_th ...$args) }
export def +thead [...args: any]: any -> list { append (_thead ...$args) }
export def +time [...args: any]: any -> list { append (_time ...$args) }
export def +title [...args: any]: any -> list { append (_title ...$args) }
export def +tr [...args: any]: any -> list { append (_tr ...$args) }
export def +track [attrs?: record]: any -> list { append (_track $attrs) }
export def +u [...args: any]: any -> list { append (_u ...$args) }
export def +ul [...args: any]: any -> list { append (_ul ...$args) }
export def +var [...args: any]: any -> list { append (_var ...$args) }
export def +video [...args: any]: any -> list { append (_video ...$args) }
export def +wbr [attrs?: record]: any -> list { append (_wbr $attrs) }

# UPPERCASE variants - same elements, shouty style
export def HTML [...args: any]: nothing -> record { render-tag html ...$args }
export def HEAD [...args: any]: nothing -> record { render-tag head ...$args }
export def TITLE [...args: any]: nothing -> record { render-tag title ...$args }
export def BASE [attrs?: record]: nothing -> record { render-void-tag base $attrs }
export def LINK [attrs?: record]: nothing -> record { render-void-tag link $attrs }
export def META [attrs?: record]: nothing -> record { render-void-tag meta $attrs }
export def STYLE [...args: any]: nothing -> record { render-tag style ...$args }
export def BODY [...args: any]: nothing -> record { render-tag body ...$args }
export def ARTICLE [...args: any]: nothing -> record { render-tag article ...$args }
export def SECTION [...args: any]: nothing -> record { render-tag section ...$args }
export def NAV [...args: any]: nothing -> record { render-tag nav ...$args }
export def ASIDE [...args: any]: nothing -> record { render-tag aside ...$args }
export def HEADER [...args: any]: nothing -> record { render-tag header ...$args }
export def FOOTER [...args: any]: nothing -> record { render-tag footer ...$args }
export def MAIN [...args: any]: nothing -> record { render-tag main ...$args }
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
export def A [...args: any]: nothing -> record { render-tag a ...$args }
export def EM [...args: any]: nothing -> record { render-tag em ...$args }
export def STRONG [...args: any]: nothing -> record { render-tag strong ...$args }
export def SMALL [...args: any]: nothing -> record { render-tag small ...$args }
export def CODE [...args: any]: nothing -> record { render-tag code ...$args }
export def SPAN [...args: any]: nothing -> record { render-tag span ...$args }
export def BR [attrs?: record]: nothing -> record { render-void-tag br $attrs }
export def WBR [attrs?: record]: nothing -> record { render-void-tag wbr $attrs }
export def IMG [attrs?: record]: nothing -> record { render-void-tag img $attrs }
export def IFRAME [...args: any]: nothing -> record { render-tag iframe ...$args }
export def EMBED [attrs?: record]: nothing -> record { render-void-tag embed $attrs }
export def VIDEO [...args: any]: nothing -> record { render-tag video ...$args }
export def AUDIO [...args: any]: nothing -> record { render-tag audio ...$args }
export def SOURCE [attrs?: record]: nothing -> record { render-void-tag source $attrs }
export def TRACK [attrs?: record]: nothing -> record { render-void-tag track $attrs }
export def CANVAS [...args: any]: nothing -> record { render-tag canvas ...$args }
export def SVG [...args: any]: nothing -> record { render-tag svg ...$args }
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
export def FORM [...args: any]: nothing -> record { render-tag form ...$args }
export def LABEL [...args: any]: nothing -> record { render-tag label ...$args }
export def INPUT [attrs?: record]: nothing -> record { render-void-tag input $attrs }
export def BUTTON [...args: any]: nothing -> record { render-tag button ...$args }
export def SELECT [...args: any]: nothing -> record { render-tag select ...$args }
export def OPTION [...args: any]: nothing -> record { render-tag option ...$args }
export def TEXTAREA [...args: any]: nothing -> record { render-tag textarea ...$args }
export def FIELDSET [...args: any]: nothing -> record { render-tag fieldset ...$args }
export def LEGEND [...args: any]: nothing -> record { render-tag legend ...$args }
export def H1 [...args: any]: nothing -> record { render-tag h1 ...$args }
export def H2 [...args: any]: nothing -> record { render-tag h2 ...$args }
export def H3 [...args: any]: nothing -> record { render-tag h3 ...$args }
export def H4 [...args: any]: nothing -> record { render-tag h4 ...$args }
export def H5 [...args: any]: nothing -> record { render-tag h5 ...$args }
export def H6 [...args: any]: nothing -> record { render-tag h6 ...$args }
export def SCRIPT [...args: any]: nothing -> record { render-tag script ...$args }
export def NOSCRIPT [...args: any]: nothing -> record { render-tag noscript ...$args }
export def TEMPLATE [...args: any]: nothing -> record { render-tag template ...$args }
export def SLOT [...args: any]: nothing -> record { render-tag slot ...$args }
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
export def DEL [...args: any]: nothing -> record { render-tag del ...$args }
export def INS [...args: any]: nothing -> record { render-tag ins ...$args }
export def DETAILS [...args: any]: nothing -> record { render-tag details ...$args }
export def SUMMARY [...args: any]: nothing -> record { render-tag summary ...$args }
export def DIALOG [...args: any]: nothing -> record { render-tag dialog ...$args }
export def ADDRESS [...args: any]: nothing -> record { render-tag address ...$args }
export def HGROUP [...args: any]: nothing -> record { render-tag hgroup ...$args }
export def SEARCH [...args: any]: nothing -> record { render-tag search ...$args }
export def MENU [...args: any]: nothing -> record { render-tag menu ...$args }
export def AREA [attrs?: record]: nothing -> record { render-void-tag area $attrs }
export def MAP [...args: any]: nothing -> record { render-tag map ...$args }
export def PICTURE [...args: any]: nothing -> record { render-tag picture ...$args }
export def METER [...args: any]: nothing -> record { render-tag meter ...$args }
export def PROGRESS [...args: any]: nothing -> record { render-tag progress ...$args }
export def OUTPUT [...args: any]: nothing -> record { render-tag output ...$args }
export def DATALIST [...args: any]: nothing -> record { render-tag datalist ...$args }
export def OPTGROUP [...args: any]: nothing -> record { render-tag optgroup ...$args }
export def RUBY [...args: any]: nothing -> record { render-tag ruby ...$args }
export def RT [...args: any]: nothing -> record { render-tag rt ...$args }
export def RP [...args: any]: nothing -> record { render-tag rp ...$args }
export def BDI [...args: any]: nothing -> record { render-tag bdi ...$args }
export def BDO [...args: any]: nothing -> record { render-tag bdo ...$args }
