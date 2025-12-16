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
def render-tag [tag: string arg1?: any arg2?: any]: nothing -> string {
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
def render-void-tag [tag: string attrs?: record]: nothing -> string {
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
