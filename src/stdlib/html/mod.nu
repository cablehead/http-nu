# HTML DSL for nushell

# {class: "foo"} -> ' class="foo"'
# style can be a record: {style: {color: red, padding: 10px}} -> ' style="color: red; padding: 10px;"'
export def attrs-to-string []: record -> string {
  $in
  | transpose key value
  | each {|attr|
    let value = if $attr.key == "style" and ($attr.value | describe -d | get type) == "record" {
      $attr.value | transpose k v | each {|p| $"($p.k): ($p.v);" } | str join " "
    } else if $attr.key == "class" and ($attr.value | describe -d | get type) == "list" {
      $attr.value | str join " "
    } else {
      $attr.value
    }
    $'($attr.key)="($value)"'
  }
  | str join ' '
  | if ($in | is-empty) { "" } else { $" ($in)" }
}

# Render a tag with optional attributes and children
export def render-tag [tag: string ...args: any]: nothing -> string {
  # Normalize to [attrs, ...content] - prepend {} if first arg isn't a record
  let args = if ($args | first | describe -d | get type) == 'record' { $args } else { $args | prepend {} }
  let attrs = $args | first
  let content = $args | skip 1

  # Normalize content to string, joining lists
  def to-children []: any -> string {
    match ($in | describe -d | get type) {
      'string' => $in
      'list' => ($in | each {to-children} | str join)
      'closure' => (do $in | to-children)
      _ => ""
    }
  }

  let children = $content | to-children
  let attrs_str = $attrs | attrs-to-string
  $"<($tag)($attrs_str)>($children)</($tag)>"
}

# Render a void tag (no children allowed per HTML spec)
export def render-void-tag [tag: string attrs?: record]: nothing -> string {
  let attrs_str = ($attrs | default {}) | attrs-to-string
  $"<($tag)($attrs_str)>"
}

# Document metadata
export def _html [...args: any]: nothing -> string { render-tag html ...$args }
export def _head [...args: any]: nothing -> string { render-tag head ...$args }
export def _title [...args: any]: nothing -> string { render-tag title ...$args }
export def _base [attrs?: record]: nothing -> string { render-void-tag base $attrs }
export def _link [attrs?: record]: nothing -> string { render-void-tag link $attrs }
export def _meta [attrs?: record]: nothing -> string { render-void-tag meta $attrs }
export def _style [...args: any]: nothing -> string { render-tag style ...$args }

# Sections
export def _body [...args: any]: nothing -> string { render-tag body ...$args }
export def _article [...args: any]: nothing -> string { render-tag article ...$args }
export def _section [...args: any]: nothing -> string { render-tag section ...$args }
export def _nav [...args: any]: nothing -> string { render-tag nav ...$args }
export def _aside [...args: any]: nothing -> string { render-tag aside ...$args }
export def _header [...args: any]: nothing -> string { render-tag header ...$args }
export def _footer [...args: any]: nothing -> string { render-tag footer ...$args }
export def _main [...args: any]: nothing -> string { render-tag main ...$args }

# Grouping content
export def _div [...args: any]: nothing -> string { render-tag div ...$args }
export def _p [...args: any]: nothing -> string { render-tag p ...$args }
export def _hr [attrs?: record]: nothing -> string { render-void-tag hr $attrs }
export def _pre [...args: any]: nothing -> string { render-tag pre ...$args }
export def _blockquote [...args: any]: nothing -> string { render-tag blockquote ...$args }
export def _ol [...args: any]: nothing -> string { render-tag ol ...$args }
export def _ul [...args: any]: nothing -> string { render-tag ul ...$args }
export def _li [...args: any]: nothing -> string { render-tag li ...$args }
export def _dl [...args: any]: nothing -> string { render-tag dl ...$args }
export def _dt [...args: any]: nothing -> string { render-tag dt ...$args }
export def _dd [...args: any]: nothing -> string { render-tag dd ...$args }
export def _figure [...args: any]: nothing -> string { render-tag figure ...$args }
export def _figcaption [...args: any]: nothing -> string { render-tag figcaption ...$args }

# Text content
export def _a [...args: any]: nothing -> string { render-tag a ...$args }
export def _em [...args: any]: nothing -> string { render-tag em ...$args }
export def _strong [...args: any]: nothing -> string { render-tag strong ...$args }
export def _small [...args: any]: nothing -> string { render-tag small ...$args }
export def _code [...args: any]: nothing -> string { render-tag code ...$args }
export def _span [...args: any]: nothing -> string { render-tag span ...$args }
export def _br [attrs?: record]: nothing -> string { render-void-tag br $attrs }
export def _wbr [attrs?: record]: nothing -> string { render-void-tag wbr $attrs }

# Embedded content
export def _img [attrs?: record]: nothing -> string { render-void-tag img $attrs }
export def _iframe [...args: any]: nothing -> string { render-tag iframe ...$args }
export def _embed [attrs?: record]: nothing -> string { render-void-tag embed $attrs }
export def _video [...args: any]: nothing -> string { render-tag video ...$args }
export def _audio [...args: any]: nothing -> string { render-tag audio ...$args }
export def _source [attrs?: record]: nothing -> string { render-void-tag source $attrs }
export def _track [attrs?: record]: nothing -> string { render-void-tag track $attrs }
export def _canvas [...args: any]: nothing -> string { render-tag canvas ...$args }
export def _svg [...args: any]: nothing -> string { render-tag svg ...$args }

# Tables
export def _table [...args: any]: nothing -> string { render-tag table ...$args }
export def _caption [...args: any]: nothing -> string { render-tag caption ...$args }
export def _thead [...args: any]: nothing -> string { render-tag thead ...$args }
export def _tbody [...args: any]: nothing -> string { render-tag tbody ...$args }
export def _tfoot [...args: any]: nothing -> string { render-tag tfoot ...$args }
export def _tr [...args: any]: nothing -> string { render-tag tr ...$args }
export def _th [...args: any]: nothing -> string { render-tag th ...$args }
export def _td [...args: any]: nothing -> string { render-tag td ...$args }
export def _col [attrs?: record]: nothing -> string { render-void-tag col $attrs }
export def _colgroup [...args: any]: nothing -> string { render-tag colgroup ...$args }

# Forms
export def _form [...args: any]: nothing -> string { render-tag form ...$args }
export def _label [...args: any]: nothing -> string { render-tag label ...$args }
export def _input [attrs?: record]: nothing -> string { render-void-tag input $attrs }
export def _button [...args: any]: nothing -> string { render-tag button ...$args }
export def _select [...args: any]: nothing -> string { render-tag select ...$args }
export def _option [...args: any]: nothing -> string { render-tag option ...$args }
export def _textarea [...args: any]: nothing -> string { render-tag textarea ...$args }
export def _fieldset [...args: any]: nothing -> string { render-tag fieldset ...$args }
export def _legend [...args: any]: nothing -> string { render-tag legend ...$args }

# Headings
export def _h1 [...args: any]: nothing -> string { render-tag h1 ...$args }
export def _h2 [...args: any]: nothing -> string { render-tag h2 ...$args }
export def _h3 [...args: any]: nothing -> string { render-tag h3 ...$args }
export def _h4 [...args: any]: nothing -> string { render-tag h4 ...$args }
export def _h5 [...args: any]: nothing -> string { render-tag h5 ...$args }
export def _h6 [...args: any]: nothing -> string { render-tag h6 ...$args }

# Scripting
export def _script [...args: any]: nothing -> string { render-tag script ...$args }
export def _noscript [...args: any]: nothing -> string { render-tag noscript ...$args }
export def _template [...args: any]: nothing -> string { render-tag template ...$args }
export def _slot [...args: any]: nothing -> string { render-tag slot ...$args }

# Additional inline text
export def _abbr [...args: any]: nothing -> string { render-tag abbr ...$args }
export def _b [...args: any]: nothing -> string { render-tag b ...$args }
export def _i [...args: any]: nothing -> string { render-tag i ...$args }
export def _u [...args: any]: nothing -> string { render-tag u ...$args }
export def _s [...args: any]: nothing -> string { render-tag s ...$args }
export def _mark [...args: any]: nothing -> string { render-tag mark ...$args }
export def _q [...args: any]: nothing -> string { render-tag q ...$args }
export def _cite [...args: any]: nothing -> string { render-tag cite ...$args }
export def _dfn [...args: any]: nothing -> string { render-tag dfn ...$args }
export def _kbd [...args: any]: nothing -> string { render-tag kbd ...$args }
export def _samp [...args: any]: nothing -> string { render-tag samp ...$args }
export def _var [...args: any]: nothing -> string { render-tag var ...$args }
export def _sub [...args: any]: nothing -> string { render-tag sub ...$args }
export def _sup [...args: any]: nothing -> string { render-tag sup ...$args }
export def _time [...args: any]: nothing -> string { render-tag time ...$args }
export def _data [...args: any]: nothing -> string { render-tag data ...$args }

# Edits
export def _del [...args: any]: nothing -> string { render-tag del ...$args }
export def _ins [...args: any]: nothing -> string { render-tag ins ...$args }

# Interactive
export def _details [...args: any]: nothing -> string { render-tag details ...$args }
export def _summary [...args: any]: nothing -> string { render-tag summary ...$args }
export def _dialog [...args: any]: nothing -> string { render-tag dialog ...$args }

# Additional content
export def _address [...args: any]: nothing -> string { render-tag address ...$args }
export def _hgroup [...args: any]: nothing -> string { render-tag hgroup ...$args }
export def _search [...args: any]: nothing -> string { render-tag search ...$args }
export def _menu [...args: any]: nothing -> string { render-tag menu ...$args }

# Image maps
export def _area [attrs?: record]: nothing -> string { render-void-tag area $attrs }
export def _map [...args: any]: nothing -> string { render-tag map ...$args }
export def _picture [...args: any]: nothing -> string { render-tag picture ...$args }

# Form additions
export def _meter [...args: any]: nothing -> string { render-tag meter ...$args }
export def _progress [...args: any]: nothing -> string { render-tag progress ...$args }
export def _output [...args: any]: nothing -> string { render-tag output ...$args }
export def _datalist [...args: any]: nothing -> string { render-tag datalist ...$args }
export def _optgroup [...args: any]: nothing -> string { render-tag optgroup ...$args }

# Ruby annotations
export def _ruby [...args: any]: nothing -> string { render-tag ruby ...$args }
export def _rt [...args: any]: nothing -> string { render-tag rt ...$args }
export def _rp [...args: any]: nothing -> string { render-tag rp ...$args }

# Bidirectional text
export def _bdi [...args: any]: nothing -> string { render-tag bdi ...$args }
export def _bdo [...args: any]: nothing -> string { render-tag bdo ...$args }

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
