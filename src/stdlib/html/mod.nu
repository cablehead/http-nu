# HTML DSL for nushell

def attrs-to-string []: record -> string {
  $in
  | transpose key value
  | each {|attr| $'($attr.key)="($attr.value)"' }
  | str join ' '
  | if ($in | is-empty) { "" } else { $" ($in)" }
}

export def parse-node-args [tag: string arg1?: any arg2?: any]: nothing -> record<tag: string, attributes: record, children: list> {
  let type1 = ($arg1 | describe)

  let attrs = if ($type1 | str starts-with 'record') { $arg1 } else { {} }
  let content = if ($type1 | str starts-with 'record') { $arg2 } else { $arg1 }

  let children = if ($content | describe) == 'string' {
    [$content]
  } else if ($content | describe) == 'closure' {
    do $content null
  } else if ($content | describe) == 'nothing' {
    []
  } else {
    []
  }

  {tag: $tag attributes: $attrs children: $children}
}

def render-node [node: record<tag: string, attributes: record, children: list>]: nothing -> string {
  let attrs_str = $node.attributes | attrs-to-string
  let children_str = $node.children | str join

  # Self-closing tags
  if $node.tag in [area base br col embed hr img input link meta source track wbr] {
    $"<($node.tag)($attrs_str)>"
  } else {
    $"<($node.tag)($attrs_str)>($children_str)</($node.tag)>"
  }
}

# Helper for self-closing tags
def void-tag [tag: string attrs?: record]: any -> list {
  let node = {tag: $tag attributes: ($attrs | default {}) children: []}
  $in | append (render-node $node)
}

# Document metadata
export def h-html [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args html $arg1 $arg2
  $in | append (render-node $node)
}

export def h-head [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args head $arg1 $arg2
  $in | append (render-node $node)
}

export def h-title [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args title $arg1 $arg2
  $in | append (render-node $node)
}

export def h-base [attrs?: record]: any -> list {
  void-tag base $attrs
}

export def h-link [attrs?: record]: any -> list {
  void-tag link $attrs
}

export def h-meta [attrs?: record]: any -> list {
  void-tag meta $attrs
}

export def h-style [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args style $arg1 $arg2
  $in | append (render-node $node)
}

# Sections
export def h-body [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args body $arg1 $arg2
  $in | append (render-node $node)
}

export def h-article [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args article $arg1 $arg2
  $in | append (render-node $node)
}

export def h-section [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args section $arg1 $arg2
  $in | append (render-node $node)
}

export def h-nav [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args nav $arg1 $arg2
  $in | append (render-node $node)
}

export def h-aside [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args aside $arg1 $arg2
  $in | append (render-node $node)
}

export def h-header [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args header $arg1 $arg2
  $in | append (render-node $node)
}

export def h-footer [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args footer $arg1 $arg2
  $in | append (render-node $node)
}

export def h-main [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args main $arg1 $arg2
  $in | append (render-node $node)
}

# Grouping content
export def h-div [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args div $arg1 $arg2
  $in | append (render-node $node)
}

export def h-p [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args p $arg1 $arg2
  $in | append (render-node $node)
}

export def h-hr [attrs?: record]: any -> list {
  void-tag hr $attrs
}

export def h-pre [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args pre $arg1 $arg2
  $in | append (render-node $node)
}

export def h-blockquote [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args blockquote $arg1 $arg2
  $in | append (render-node $node)
}

export def h-ol [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args ol $arg1 $arg2
  $in | append (render-node $node)
}

export def h-ul [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args ul $arg1 $arg2
  $in | append (render-node $node)
}

export def h-li [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args li $arg1 $arg2
  $in | append (render-node $node)
}

export def h-dl [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args dl $arg1 $arg2
  $in | append (render-node $node)
}

export def h-dt [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args dt $arg1 $arg2
  $in | append (render-node $node)
}

export def h-dd [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args dd $arg1 $arg2
  $in | append (render-node $node)
}

export def h-figure [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args figure $arg1 $arg2
  $in | append (render-node $node)
}

export def h-figcaption [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args figcaption $arg1 $arg2
  $in | append (render-node $node)
}

# Text content
export def h-a [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args a $arg1 $arg2
  $in | append (render-node $node)
}

export def h-em [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args em $arg1 $arg2
  $in | append (render-node $node)
}

export def h-strong [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args strong $arg1 $arg2
  $in | append (render-node $node)
}

export def h-small [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args small $arg1 $arg2
  $in | append (render-node $node)
}

export def h-code [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args code $arg1 $arg2
  $in | append (render-node $node)
}

export def h-span [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args span $arg1 $arg2
  $in | append (render-node $node)
}

export def h-br [attrs?: record]: any -> list {
  void-tag br $attrs
}

export def h-wbr [attrs?: record]: any -> list {
  void-tag wbr $attrs
}

# Embedded content
export def h-img [attrs?: record]: any -> list {
  void-tag img $attrs
}

export def h-iframe [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args iframe $arg1 $arg2
  $in | append (render-node $node)
}

export def h-embed [attrs?: record]: any -> list {
  void-tag embed $attrs
}

export def h-video [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args video $arg1 $arg2
  $in | append (render-node $node)
}

export def h-audio [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args audio $arg1 $arg2
  $in | append (render-node $node)
}

export def h-source [attrs?: record]: any -> list {
  void-tag source $attrs
}

export def h-track [attrs?: record]: any -> list {
  void-tag track $attrs
}

export def h-canvas [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args canvas $arg1 $arg2
  $in | append (render-node $node)
}

export def h-svg [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args svg $arg1 $arg2
  $in | append (render-node $node)
}

# Tables
export def h-table [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args table $arg1 $arg2
  $in | append (render-node $node)
}

export def h-caption [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args caption $arg1 $arg2
  $in | append (render-node $node)
}

export def h-thead [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args thead $arg1 $arg2
  $in | append (render-node $node)
}

export def h-tbody [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args tbody $arg1 $arg2
  $in | append (render-node $node)
}

export def h-tfoot [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args tfoot $arg1 $arg2
  $in | append (render-node $node)
}

export def h-tr [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args tr $arg1 $arg2
  $in | append (render-node $node)
}

export def h-th [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args th $arg1 $arg2
  $in | append (render-node $node)
}

export def h-td [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args td $arg1 $arg2
  $in | append (render-node $node)
}

export def h-col [attrs?: record]: any -> list {
  void-tag col $attrs
}

export def h-colgroup [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args colgroup $arg1 $arg2
  $in | append (render-node $node)
}

# Forms
export def h-form [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args form $arg1 $arg2
  $in | append (render-node $node)
}

export def h-label [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args label $arg1 $arg2
  $in | append (render-node $node)
}

export def h-input [attrs?: record]: any -> list {
  void-tag input $attrs
}

export def h-button [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args button $arg1 $arg2
  $in | append (render-node $node)
}

export def h-select [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args select $arg1 $arg2
  $in | append (render-node $node)
}

export def h-option [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args option $arg1 $arg2
  $in | append (render-node $node)
}

export def h-textarea [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args textarea $arg1 $arg2
  $in | append (render-node $node)
}

export def h-fieldset [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args fieldset $arg1 $arg2
  $in | append (render-node $node)
}

export def h-legend [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args legend $arg1 $arg2
  $in | append (render-node $node)
}

# Headings
export def h-h1 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h1 $arg1 $arg2
  $in | append (render-node $node)
}

export def h-h2 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h2 $arg1 $arg2
  $in | append (render-node $node)
}

export def h-h3 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h3 $arg1 $arg2
  $in | append (render-node $node)
}

export def h-h4 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h4 $arg1 $arg2
  $in | append (render-node $node)
}

export def h-h5 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h5 $arg1 $arg2
  $in | append (render-node $node)
}

export def h-h6 [arg1?: any arg2?: any]: any -> list {
  let node = parse-node-args h6 $arg1 $arg2
  $in | append (render-node $node)
}
