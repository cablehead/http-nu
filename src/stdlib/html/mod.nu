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
  let type1 = ($arg1 | describe)
  let attrs = if ($type1 | str starts-with 'record') { $arg1 } else { {} }
  let content = if ($type1 | str starts-with 'record') { $arg2 } else { $arg1 }

  let children = if ($content | describe) == 'string' {
    $content
  } else if ($content | describe) == 'closure' {
    do $content null
  } else {
    ""
  }

  let attrs_str = $attrs | attrs-to-string
  $"<($tag)($attrs_str)>($children)</($tag)>"
}

# Render a void tag (no children allowed per HTML spec)
def render-void-tag [tag: string attrs?: record]: nothing -> string {
  let attrs_str = ($attrs | default {}) | attrs-to-string
  $"<($tag)($attrs_str)>"
}

# Document metadata
export def h-html [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag html $arg1 $arg2) }
export def h-head [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag head $arg1 $arg2) }
export def h-title [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag title $arg1 $arg2) }
export def h-base [attrs?: record]: any -> string { ($in | default "") + (render-void-tag base $attrs) }
export def h-link [attrs?: record]: any -> string { ($in | default "") + (render-void-tag link $attrs) }
export def h-meta [attrs?: record]: any -> string { ($in | default "") + (render-void-tag meta $attrs) }
export def h-style [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag style $arg1 $arg2) }

# Sections
export def h-body [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag body $arg1 $arg2) }
export def h-article [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag article $arg1 $arg2) }
export def h-section [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag section $arg1 $arg2) }
export def h-nav [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag nav $arg1 $arg2) }
export def h-aside [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag aside $arg1 $arg2) }
export def h-header [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag header $arg1 $arg2) }
export def h-footer [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag footer $arg1 $arg2) }
export def h-main [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag main $arg1 $arg2) }

# Grouping content
export def h-div [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag div $arg1 $arg2) }
export def h-p [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag p $arg1 $arg2) }
export def h-hr [attrs?: record]: any -> string { ($in | default "") + (render-void-tag hr $attrs) }
export def h-pre [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag pre $arg1 $arg2) }
export def h-blockquote [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag blockquote $arg1 $arg2) }
export def h-ol [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag ol $arg1 $arg2) }
export def h-ul [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag ul $arg1 $arg2) }
export def h-li [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag li $arg1 $arg2) }
export def h-dl [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag dl $arg1 $arg2) }
export def h-dt [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag dt $arg1 $arg2) }
export def h-dd [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag dd $arg1 $arg2) }
export def h-figure [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag figure $arg1 $arg2) }
export def h-figcaption [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag figcaption $arg1 $arg2) }

# Text content
export def h-a [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag a $arg1 $arg2) }
export def h-em [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag em $arg1 $arg2) }
export def h-strong [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag strong $arg1 $arg2) }
export def h-small [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag small $arg1 $arg2) }
export def h-code [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag code $arg1 $arg2) }
export def h-span [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag span $arg1 $arg2) }
export def h-br [attrs?: record]: any -> string { ($in | default "") + (render-void-tag br $attrs) }
export def h-wbr [attrs?: record]: any -> string { ($in | default "") + (render-void-tag wbr $attrs) }

# Embedded content
export def h-img [attrs?: record]: any -> string { ($in | default "") + (render-void-tag img $attrs) }
export def h-iframe [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag iframe $arg1 $arg2) }
export def h-embed [attrs?: record]: any -> string { ($in | default "") + (render-void-tag embed $attrs) }
export def h-video [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag video $arg1 $arg2) }
export def h-audio [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag audio $arg1 $arg2) }
export def h-source [attrs?: record]: any -> string { ($in | default "") + (render-void-tag source $attrs) }
export def h-track [attrs?: record]: any -> string { ($in | default "") + (render-void-tag track $attrs) }
export def h-canvas [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag canvas $arg1 $arg2) }
export def h-svg [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag svg $arg1 $arg2) }

# Tables
export def h-table [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag table $arg1 $arg2) }
export def h-caption [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag caption $arg1 $arg2) }
export def h-thead [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag thead $arg1 $arg2) }
export def h-tbody [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag tbody $arg1 $arg2) }
export def h-tfoot [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag tfoot $arg1 $arg2) }
export def h-tr [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag tr $arg1 $arg2) }
export def h-th [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag th $arg1 $arg2) }
export def h-td [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag td $arg1 $arg2) }
export def h-col [attrs?: record]: any -> string { ($in | default "") + (render-void-tag col $attrs) }
export def h-colgroup [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag colgroup $arg1 $arg2) }

# Forms
export def h-form [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag form $arg1 $arg2) }
export def h-label [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag label $arg1 $arg2) }
export def h-input [attrs?: record]: any -> string { ($in | default "") + (render-void-tag input $attrs) }
export def h-button [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag button $arg1 $arg2) }
export def h-select [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag select $arg1 $arg2) }
export def h-option [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag option $arg1 $arg2) }
export def h-textarea [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag textarea $arg1 $arg2) }
export def h-fieldset [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag fieldset $arg1 $arg2) }
export def h-legend [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag legend $arg1 $arg2) }

# Headings
export def h-h1 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h1 $arg1 $arg2) }
export def h-h2 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h2 $arg1 $arg2) }
export def h-h3 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h3 $arg1 $arg2) }
export def h-h4 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h4 $arg1 $arg2) }
export def h-h5 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h5 $arg1 $arg2) }
export def h-h6 [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag h6 $arg1 $arg2) }

# Scripting
export def h-script [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag script $arg1 $arg2) }
export def h-noscript [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag noscript $arg1 $arg2) }
export def h-template [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag template $arg1 $arg2) }
export def h-slot [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag slot $arg1 $arg2) }

# Additional inline text
export def h-abbr [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag abbr $arg1 $arg2) }
export def h-b [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag b $arg1 $arg2) }
export def h-i [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag i $arg1 $arg2) }
export def h-u [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag u $arg1 $arg2) }
export def h-s [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag s $arg1 $arg2) }
export def h-mark [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag mark $arg1 $arg2) }
export def h-q [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag q $arg1 $arg2) }
export def h-cite [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag cite $arg1 $arg2) }
export def h-dfn [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag dfn $arg1 $arg2) }
export def h-kbd [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag kbd $arg1 $arg2) }
export def h-samp [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag samp $arg1 $arg2) }
export def h-var [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag var $arg1 $arg2) }
export def h-sub [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag sub $arg1 $arg2) }
export def h-sup [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag sup $arg1 $arg2) }
export def h-time [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag time $arg1 $arg2) }
export def h-data [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag data $arg1 $arg2) }

# Edits
export def h-del [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag del $arg1 $arg2) }
export def h-ins [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag ins $arg1 $arg2) }

# Interactive
export def h-details [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag details $arg1 $arg2) }
export def h-summary [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag summary $arg1 $arg2) }
export def h-dialog [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag dialog $arg1 $arg2) }

# Additional content
export def h-address [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag address $arg1 $arg2) }
export def h-hgroup [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag hgroup $arg1 $arg2) }
export def h-search [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag search $arg1 $arg2) }
export def h-menu [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag menu $arg1 $arg2) }

# Image maps
export def h-area [attrs?: record]: any -> string { ($in | default "") + (render-void-tag area $attrs) }
export def h-map [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag map $arg1 $arg2) }
export def h-picture [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag picture $arg1 $arg2) }

# Form additions
export def h-meter [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag meter $arg1 $arg2) }
export def h-progress [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag progress $arg1 $arg2) }
export def h-output [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag output $arg1 $arg2) }
export def h-datalist [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag datalist $arg1 $arg2) }
export def h-optgroup [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag optgroup $arg1 $arg2) }

# Ruby annotations
export def h-ruby [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag ruby $arg1 $arg2) }
export def h-rt [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag rt $arg1 $arg2) }
export def h-rp [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag rp $arg1 $arg2) }

# Bidirectional text
export def h-bdi [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag bdi $arg1 $arg2) }
export def h-bdo [arg1?: any arg2?: any]: any -> string { ($in | default "") + (render-tag bdo $arg1 $arg2) }
