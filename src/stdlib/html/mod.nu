# HTML DSL for nushell

# {class: "foo"} -> ' class="foo"'
export def attrs-to-string []: record -> string {
  $in
  | transpose key value
  | each {|attr| $'($attr.key)="($attr.value)"' }
  | str join ' '
  | if ($in | is-empty) { "" } else { $" ($in)" }
}

# Render a tag with optional attributes and children
export def render-tag [tag: string arg1?: any arg2?: any]: nothing -> string {
  let type1 = ($arg1 | describe -d | get type)
  let attrs = if $type1 == 'record' { $arg1 } else { {} }
  let content = if $type1 == 'record' { $arg2 } else { $arg1 }

  # Normalize content to string, joining lists
  def to-children []: any -> string {
    match ($in | describe -d | get type) {
      'string' => $in
      'list' => ($in | str join)
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
export def _html [arg1?: any arg2?: any]: nothing -> string { render-tag html $arg1 $arg2 }
export def _head [arg1?: any arg2?: any]: nothing -> string { render-tag head $arg1 $arg2 }
export def _title [arg1?: any arg2?: any]: nothing -> string { render-tag title $arg1 $arg2 }
export def _base [attrs?: record]: nothing -> string { render-void-tag base $attrs }
export def _link [attrs?: record]: nothing -> string { render-void-tag link $attrs }
export def _meta [attrs?: record]: nothing -> string { render-void-tag meta $attrs }
export def _style [arg1?: any arg2?: any]: nothing -> string { render-tag style $arg1 $arg2 }

# Sections
export def _body [arg1?: any arg2?: any]: nothing -> string { render-tag body $arg1 $arg2 }
export def _article [arg1?: any arg2?: any]: nothing -> string { render-tag article $arg1 $arg2 }
export def _section [arg1?: any arg2?: any]: nothing -> string { render-tag section $arg1 $arg2 }
export def _nav [arg1?: any arg2?: any]: nothing -> string { render-tag nav $arg1 $arg2 }
export def _aside [arg1?: any arg2?: any]: nothing -> string { render-tag aside $arg1 $arg2 }
export def _header [arg1?: any arg2?: any]: nothing -> string { render-tag header $arg1 $arg2 }
export def _footer [arg1?: any arg2?: any]: nothing -> string { render-tag footer $arg1 $arg2 }
export def _main [arg1?: any arg2?: any]: nothing -> string { render-tag main $arg1 $arg2 }

# Grouping content
export def _div [arg1?: any arg2?: any]: nothing -> string { render-tag div $arg1 $arg2 }
export def _p [arg1?: any arg2?: any]: nothing -> string { render-tag p $arg1 $arg2 }
export def _hr [attrs?: record]: nothing -> string { render-void-tag hr $attrs }
export def _pre [arg1?: any arg2?: any]: nothing -> string { render-tag pre $arg1 $arg2 }
export def _blockquote [arg1?: any arg2?: any]: nothing -> string { render-tag blockquote $arg1 $arg2 }
export def _ol [arg1?: any arg2?: any]: nothing -> string { render-tag ol $arg1 $arg2 }
export def _ul [arg1?: any arg2?: any]: nothing -> string { render-tag ul $arg1 $arg2 }
export def _li [arg1?: any arg2?: any]: nothing -> string { render-tag li $arg1 $arg2 }
export def _dl [arg1?: any arg2?: any]: nothing -> string { render-tag dl $arg1 $arg2 }
export def _dt [arg1?: any arg2?: any]: nothing -> string { render-tag dt $arg1 $arg2 }
export def _dd [arg1?: any arg2?: any]: nothing -> string { render-tag dd $arg1 $arg2 }
export def _figure [arg1?: any arg2?: any]: nothing -> string { render-tag figure $arg1 $arg2 }
export def _figcaption [arg1?: any arg2?: any]: nothing -> string { render-tag figcaption $arg1 $arg2 }

# Text content
export def _a [arg1?: any arg2?: any]: nothing -> string { render-tag a $arg1 $arg2 }
export def _em [arg1?: any arg2?: any]: nothing -> string { render-tag em $arg1 $arg2 }
export def _strong [arg1?: any arg2?: any]: nothing -> string { render-tag strong $arg1 $arg2 }
export def _small [arg1?: any arg2?: any]: nothing -> string { render-tag small $arg1 $arg2 }
export def _code [arg1?: any arg2?: any]: nothing -> string { render-tag code $arg1 $arg2 }
export def _span [arg1?: any arg2?: any]: nothing -> string { render-tag span $arg1 $arg2 }
export def _br [attrs?: record]: nothing -> string { render-void-tag br $attrs }
export def _wbr [attrs?: record]: nothing -> string { render-void-tag wbr $attrs }

# Embedded content
export def _img [attrs?: record]: nothing -> string { render-void-tag img $attrs }
export def _iframe [arg1?: any arg2?: any]: nothing -> string { render-tag iframe $arg1 $arg2 }
export def _embed [attrs?: record]: nothing -> string { render-void-tag embed $attrs }
export def _video [arg1?: any arg2?: any]: nothing -> string { render-tag video $arg1 $arg2 }
export def _audio [arg1?: any arg2?: any]: nothing -> string { render-tag audio $arg1 $arg2 }
export def _source [attrs?: record]: nothing -> string { render-void-tag source $attrs }
export def _track [attrs?: record]: nothing -> string { render-void-tag track $attrs }
export def _canvas [arg1?: any arg2?: any]: nothing -> string { render-tag canvas $arg1 $arg2 }
export def _svg [arg1?: any arg2?: any]: nothing -> string { render-tag svg $arg1 $arg2 }

# Tables
export def _table [arg1?: any arg2?: any]: nothing -> string { render-tag table $arg1 $arg2 }
export def _caption [arg1?: any arg2?: any]: nothing -> string { render-tag caption $arg1 $arg2 }
export def _thead [arg1?: any arg2?: any]: nothing -> string { render-tag thead $arg1 $arg2 }
export def _tbody [arg1?: any arg2?: any]: nothing -> string { render-tag tbody $arg1 $arg2 }
export def _tfoot [arg1?: any arg2?: any]: nothing -> string { render-tag tfoot $arg1 $arg2 }
export def _tr [arg1?: any arg2?: any]: nothing -> string { render-tag tr $arg1 $arg2 }
export def _th [arg1?: any arg2?: any]: nothing -> string { render-tag th $arg1 $arg2 }
export def _td [arg1?: any arg2?: any]: nothing -> string { render-tag td $arg1 $arg2 }
export def _col [attrs?: record]: nothing -> string { render-void-tag col $attrs }
export def _colgroup [arg1?: any arg2?: any]: nothing -> string { render-tag colgroup $arg1 $arg2 }

# Forms
export def _form [arg1?: any arg2?: any]: nothing -> string { render-tag form $arg1 $arg2 }
export def _label [arg1?: any arg2?: any]: nothing -> string { render-tag label $arg1 $arg2 }
export def _input [attrs?: record]: nothing -> string { render-void-tag input $attrs }
export def _button [arg1?: any arg2?: any]: nothing -> string { render-tag button $arg1 $arg2 }
export def _select [arg1?: any arg2?: any]: nothing -> string { render-tag select $arg1 $arg2 }
export def _option [arg1?: any arg2?: any]: nothing -> string { render-tag option $arg1 $arg2 }
export def _textarea [arg1?: any arg2?: any]: nothing -> string { render-tag textarea $arg1 $arg2 }
export def _fieldset [arg1?: any arg2?: any]: nothing -> string { render-tag fieldset $arg1 $arg2 }
export def _legend [arg1?: any arg2?: any]: nothing -> string { render-tag legend $arg1 $arg2 }

# Headings
export def _h1 [arg1?: any arg2?: any]: nothing -> string { render-tag h1 $arg1 $arg2 }
export def _h2 [arg1?: any arg2?: any]: nothing -> string { render-tag h2 $arg1 $arg2 }
export def _h3 [arg1?: any arg2?: any]: nothing -> string { render-tag h3 $arg1 $arg2 }
export def _h4 [arg1?: any arg2?: any]: nothing -> string { render-tag h4 $arg1 $arg2 }
export def _h5 [arg1?: any arg2?: any]: nothing -> string { render-tag h5 $arg1 $arg2 }
export def _h6 [arg1?: any arg2?: any]: nothing -> string { render-tag h6 $arg1 $arg2 }

# Scripting
export def _script [arg1?: any arg2?: any]: nothing -> string { render-tag script $arg1 $arg2 }
export def _noscript [arg1?: any arg2?: any]: nothing -> string { render-tag noscript $arg1 $arg2 }
export def _template [arg1?: any arg2?: any]: nothing -> string { render-tag template $arg1 $arg2 }
export def _slot [arg1?: any arg2?: any]: nothing -> string { render-tag slot $arg1 $arg2 }

# Additional inline text
export def _abbr [arg1?: any arg2?: any]: nothing -> string { render-tag abbr $arg1 $arg2 }
export def _b [arg1?: any arg2?: any]: nothing -> string { render-tag b $arg1 $arg2 }
export def _i [arg1?: any arg2?: any]: nothing -> string { render-tag i $arg1 $arg2 }
export def _u [arg1?: any arg2?: any]: nothing -> string { render-tag u $arg1 $arg2 }
export def _s [arg1?: any arg2?: any]: nothing -> string { render-tag s $arg1 $arg2 }
export def _mark [arg1?: any arg2?: any]: nothing -> string { render-tag mark $arg1 $arg2 }
export def _q [arg1?: any arg2?: any]: nothing -> string { render-tag q $arg1 $arg2 }
export def _cite [arg1?: any arg2?: any]: nothing -> string { render-tag cite $arg1 $arg2 }
export def _dfn [arg1?: any arg2?: any]: nothing -> string { render-tag dfn $arg1 $arg2 }
export def _kbd [arg1?: any arg2?: any]: nothing -> string { render-tag kbd $arg1 $arg2 }
export def _samp [arg1?: any arg2?: any]: nothing -> string { render-tag samp $arg1 $arg2 }
export def _var [arg1?: any arg2?: any]: nothing -> string { render-tag var $arg1 $arg2 }
export def _sub [arg1?: any arg2?: any]: nothing -> string { render-tag sub $arg1 $arg2 }
export def _sup [arg1?: any arg2?: any]: nothing -> string { render-tag sup $arg1 $arg2 }
export def _time [arg1?: any arg2?: any]: nothing -> string { render-tag time $arg1 $arg2 }
export def _data [arg1?: any arg2?: any]: nothing -> string { render-tag data $arg1 $arg2 }

# Edits
export def _del [arg1?: any arg2?: any]: nothing -> string { render-tag del $arg1 $arg2 }
export def _ins [arg1?: any arg2?: any]: nothing -> string { render-tag ins $arg1 $arg2 }

# Interactive
export def _details [arg1?: any arg2?: any]: nothing -> string { render-tag details $arg1 $arg2 }
export def _summary [arg1?: any arg2?: any]: nothing -> string { render-tag summary $arg1 $arg2 }
export def _dialog [arg1?: any arg2?: any]: nothing -> string { render-tag dialog $arg1 $arg2 }

# Additional content
export def _address [arg1?: any arg2?: any]: nothing -> string { render-tag address $arg1 $arg2 }
export def _hgroup [arg1?: any arg2?: any]: nothing -> string { render-tag hgroup $arg1 $arg2 }
export def _search [arg1?: any arg2?: any]: nothing -> string { render-tag search $arg1 $arg2 }
export def _menu [arg1?: any arg2?: any]: nothing -> string { render-tag menu $arg1 $arg2 }

# Image maps
export def _area [attrs?: record]: nothing -> string { render-void-tag area $attrs }
export def _map [arg1?: any arg2?: any]: nothing -> string { render-tag map $arg1 $arg2 }
export def _picture [arg1?: any arg2?: any]: nothing -> string { render-tag picture $arg1 $arg2 }

# Form additions
export def _meter [arg1?: any arg2?: any]: nothing -> string { render-tag meter $arg1 $arg2 }
export def _progress [arg1?: any arg2?: any]: nothing -> string { render-tag progress $arg1 $arg2 }
export def _output [arg1?: any arg2?: any]: nothing -> string { render-tag output $arg1 $arg2 }
export def _datalist [arg1?: any arg2?: any]: nothing -> string { render-tag datalist $arg1 $arg2 }
export def _optgroup [arg1?: any arg2?: any]: nothing -> string { render-tag optgroup $arg1 $arg2 }

# Ruby annotations
export def _ruby [arg1?: any arg2?: any]: nothing -> string { render-tag ruby $arg1 $arg2 }
export def _rt [arg1?: any arg2?: any]: nothing -> string { render-tag rt $arg1 $arg2 }
export def _rp [arg1?: any arg2?: any]: nothing -> string { render-tag rp $arg1 $arg2 }

# Bidirectional text
export def _bdi [arg1?: any arg2?: any]: nothing -> string { render-tag bdi $arg1 $arg2 }
export def _bdo [arg1?: any arg2?: any]: nothing -> string { render-tag bdo $arg1 $arg2 }

# Append variants (+tag) - pipe-friendly siblings: _div "x" | +p "y" => [<div>x</div> <p>y</p>]
export def +a [arg1?: any arg2?: any]: any -> list { append (_a $arg1 $arg2) }
export def +abbr [arg1?: any arg2?: any]: any -> list { append (_abbr $arg1 $arg2) }
export def +address [arg1?: any arg2?: any]: any -> list { append (_address $arg1 $arg2) }
export def +area [attrs?: record]: any -> list { append (_area $attrs) }
export def +article [arg1?: any arg2?: any]: any -> list { append (_article $arg1 $arg2) }
export def +aside [arg1?: any arg2?: any]: any -> list { append (_aside $arg1 $arg2) }
export def +audio [arg1?: any arg2?: any]: any -> list { append (_audio $arg1 $arg2) }
export def +b [arg1?: any arg2?: any]: any -> list { append (_b $arg1 $arg2) }
export def +base [attrs?: record]: any -> list { append (_base $attrs) }
export def +bdi [arg1?: any arg2?: any]: any -> list { append (_bdi $arg1 $arg2) }
export def +bdo [arg1?: any arg2?: any]: any -> list { append (_bdo $arg1 $arg2) }
export def +blockquote [arg1?: any arg2?: any]: any -> list { append (_blockquote $arg1 $arg2) }
export def +body [arg1?: any arg2?: any]: any -> list { append (_body $arg1 $arg2) }
export def +br [attrs?: record]: any -> list { append (_br $attrs) }
export def +button [arg1?: any arg2?: any]: any -> list { append (_button $arg1 $arg2) }
export def +canvas [arg1?: any arg2?: any]: any -> list { append (_canvas $arg1 $arg2) }
export def +caption [arg1?: any arg2?: any]: any -> list { append (_caption $arg1 $arg2) }
export def +cite [arg1?: any arg2?: any]: any -> list { append (_cite $arg1 $arg2) }
export def +code [arg1?: any arg2?: any]: any -> list { append (_code $arg1 $arg2) }
export def +col [attrs?: record]: any -> list { append (_col $attrs) }
export def +colgroup [arg1?: any arg2?: any]: any -> list { append (_colgroup $arg1 $arg2) }
export def +data [arg1?: any arg2?: any]: any -> list { append (_data $arg1 $arg2) }
export def +datalist [arg1?: any arg2?: any]: any -> list { append (_datalist $arg1 $arg2) }
export def +dd [arg1?: any arg2?: any]: any -> list { append (_dd $arg1 $arg2) }
export def +del [arg1?: any arg2?: any]: any -> list { append (_del $arg1 $arg2) }
export def +details [arg1?: any arg2?: any]: any -> list { append (_details $arg1 $arg2) }
export def +dfn [arg1?: any arg2?: any]: any -> list { append (_dfn $arg1 $arg2) }
export def +dialog [arg1?: any arg2?: any]: any -> list { append (_dialog $arg1 $arg2) }
export def +div [arg1?: any arg2?: any]: any -> list { append (_div $arg1 $arg2) }
export def +dl [arg1?: any arg2?: any]: any -> list { append (_dl $arg1 $arg2) }
export def +dt [arg1?: any arg2?: any]: any -> list { append (_dt $arg1 $arg2) }
export def +em [arg1?: any arg2?: any]: any -> list { append (_em $arg1 $arg2) }
export def +embed [attrs?: record]: any -> list { append (_embed $attrs) }
export def +fieldset [arg1?: any arg2?: any]: any -> list { append (_fieldset $arg1 $arg2) }
export def +figcaption [arg1?: any arg2?: any]: any -> list { append (_figcaption $arg1 $arg2) }
export def +figure [arg1?: any arg2?: any]: any -> list { append (_figure $arg1 $arg2) }
export def +footer [arg1?: any arg2?: any]: any -> list { append (_footer $arg1 $arg2) }
export def +form [arg1?: any arg2?: any]: any -> list { append (_form $arg1 $arg2) }
export def +h1 [arg1?: any arg2?: any]: any -> list { append (_h1 $arg1 $arg2) }
export def +h2 [arg1?: any arg2?: any]: any -> list { append (_h2 $arg1 $arg2) }
export def +h3 [arg1?: any arg2?: any]: any -> list { append (_h3 $arg1 $arg2) }
export def +h4 [arg1?: any arg2?: any]: any -> list { append (_h4 $arg1 $arg2) }
export def +h5 [arg1?: any arg2?: any]: any -> list { append (_h5 $arg1 $arg2) }
export def +h6 [arg1?: any arg2?: any]: any -> list { append (_h6 $arg1 $arg2) }
export def +head [arg1?: any arg2?: any]: any -> list { append (_head $arg1 $arg2) }
export def +header [arg1?: any arg2?: any]: any -> list { append (_header $arg1 $arg2) }
export def +hgroup [arg1?: any arg2?: any]: any -> list { append (_hgroup $arg1 $arg2) }
export def +hr [attrs?: record]: any -> list { append (_hr $attrs) }
export def +html [arg1?: any arg2?: any]: any -> list { append (_html $arg1 $arg2) }
export def +i [arg1?: any arg2?: any]: any -> list { append (_i $arg1 $arg2) }
export def +iframe [arg1?: any arg2?: any]: any -> list { append (_iframe $arg1 $arg2) }
export def +img [attrs?: record]: any -> list { append (_img $attrs) }
export def +input [attrs?: record]: any -> list { append (_input $attrs) }
export def +ins [arg1?: any arg2?: any]: any -> list { append (_ins $arg1 $arg2) }
export def +kbd [arg1?: any arg2?: any]: any -> list { append (_kbd $arg1 $arg2) }
export def +label [arg1?: any arg2?: any]: any -> list { append (_label $arg1 $arg2) }
export def +legend [arg1?: any arg2?: any]: any -> list { append (_legend $arg1 $arg2) }
export def +li [arg1?: any arg2?: any]: any -> list { append (_li $arg1 $arg2) }
export def +link [attrs?: record]: any -> list { append (_link $attrs) }
export def +main [arg1?: any arg2?: any]: any -> list { append (_main $arg1 $arg2) }
export def +map [arg1?: any arg2?: any]: any -> list { append (_map $arg1 $arg2) }
export def +mark [arg1?: any arg2?: any]: any -> list { append (_mark $arg1 $arg2) }
export def +menu [arg1?: any arg2?: any]: any -> list { append (_menu $arg1 $arg2) }
export def +meta [attrs?: record]: any -> list { append (_meta $attrs) }
export def +meter [arg1?: any arg2?: any]: any -> list { append (_meter $arg1 $arg2) }
export def +nav [arg1?: any arg2?: any]: any -> list { append (_nav $arg1 $arg2) }
export def +noscript [arg1?: any arg2?: any]: any -> list { append (_noscript $arg1 $arg2) }
export def +ol [arg1?: any arg2?: any]: any -> list { append (_ol $arg1 $arg2) }
export def +optgroup [arg1?: any arg2?: any]: any -> list { append (_optgroup $arg1 $arg2) }
export def +option [arg1?: any arg2?: any]: any -> list { append (_option $arg1 $arg2) }
export def +output [arg1?: any arg2?: any]: any -> list { append (_output $arg1 $arg2) }
export def +p [arg1?: any arg2?: any]: any -> list { append (_p $arg1 $arg2) }
export def +picture [arg1?: any arg2?: any]: any -> list { append (_picture $arg1 $arg2) }
export def +pre [arg1?: any arg2?: any]: any -> list { append (_pre $arg1 $arg2) }
export def +progress [arg1?: any arg2?: any]: any -> list { append (_progress $arg1 $arg2) }
export def +q [arg1?: any arg2?: any]: any -> list { append (_q $arg1 $arg2) }
export def +rp [arg1?: any arg2?: any]: any -> list { append (_rp $arg1 $arg2) }
export def +rt [arg1?: any arg2?: any]: any -> list { append (_rt $arg1 $arg2) }
export def +ruby [arg1?: any arg2?: any]: any -> list { append (_ruby $arg1 $arg2) }
export def +s [arg1?: any arg2?: any]: any -> list { append (_s $arg1 $arg2) }
export def +samp [arg1?: any arg2?: any]: any -> list { append (_samp $arg1 $arg2) }
export def +script [arg1?: any arg2?: any]: any -> list { append (_script $arg1 $arg2) }
export def +search [arg1?: any arg2?: any]: any -> list { append (_search $arg1 $arg2) }
export def +section [arg1?: any arg2?: any]: any -> list { append (_section $arg1 $arg2) }
export def +select [arg1?: any arg2?: any]: any -> list { append (_select $arg1 $arg2) }
export def +slot [arg1?: any arg2?: any]: any -> list { append (_slot $arg1 $arg2) }
export def +small [arg1?: any arg2?: any]: any -> list { append (_small $arg1 $arg2) }
export def +source [attrs?: record]: any -> list { append (_source $attrs) }
export def +span [arg1?: any arg2?: any]: any -> list { append (_span $arg1 $arg2) }
export def +strong [arg1?: any arg2?: any]: any -> list { append (_strong $arg1 $arg2) }
export def +style [arg1?: any arg2?: any]: any -> list { append (_style $arg1 $arg2) }
export def +sub [arg1?: any arg2?: any]: any -> list { append (_sub $arg1 $arg2) }
export def +summary [arg1?: any arg2?: any]: any -> list { append (_summary $arg1 $arg2) }
export def +sup [arg1?: any arg2?: any]: any -> list { append (_sup $arg1 $arg2) }
export def +svg [arg1?: any arg2?: any]: any -> list { append (_svg $arg1 $arg2) }
export def +table [arg1?: any arg2?: any]: any -> list { append (_table $arg1 $arg2) }
export def +tbody [arg1?: any arg2?: any]: any -> list { append (_tbody $arg1 $arg2) }
export def +td [arg1?: any arg2?: any]: any -> list { append (_td $arg1 $arg2) }
export def +template [arg1?: any arg2?: any]: any -> list { append (_template $arg1 $arg2) }
export def +textarea [arg1?: any arg2?: any]: any -> list { append (_textarea $arg1 $arg2) }
export def +tfoot [arg1?: any arg2?: any]: any -> list { append (_tfoot $arg1 $arg2) }
export def +th [arg1?: any arg2?: any]: any -> list { append (_th $arg1 $arg2) }
export def +thead [arg1?: any arg2?: any]: any -> list { append (_thead $arg1 $arg2) }
export def +time [arg1?: any arg2?: any]: any -> list { append (_time $arg1 $arg2) }
export def +title [arg1?: any arg2?: any]: any -> list { append (_title $arg1 $arg2) }
export def +tr [arg1?: any arg2?: any]: any -> list { append (_tr $arg1 $arg2) }
export def +track [attrs?: record]: any -> list { append (_track $attrs) }
export def +u [arg1?: any arg2?: any]: any -> list { append (_u $arg1 $arg2) }
export def +ul [arg1?: any arg2?: any]: any -> list { append (_ul $arg1 $arg2) }
export def +var [arg1?: any arg2?: any]: any -> list { append (_var $arg1 $arg2) }
export def +video [arg1?: any arg2?: any]: any -> list { append (_video $arg1 $arg2) }
export def +wbr [attrs?: record]: any -> list { append (_wbr $attrs) }
