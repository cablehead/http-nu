/**
 * Upgrade mermaid code fences to live diagrams.
 *
 * http-nu's `.md` renders a ```mermaid fence as:
 *
 *   <pre><code class="language-mermaid">...</code></pre>
 *
 * This script finds each of those blocks, reads the diagram source via
 * textContent (which drops the highlighter's spans and decodes entities
 * such as &gt;), and replaces the <pre> with a <mermaid-diagram> element.
 * The web component in mermaid-diagram.js does the actual rendering.
 */

import './mermaid-diagram.js';

function upgrade(root = document) {
  for (const code of root.querySelectorAll('pre > code.language-mermaid')) {
    const pre = code.parentElement;
    const el = document.createElement('mermaid-diagram');
    el.textContent = code.textContent;
    pre.replaceWith(el);
  }
}

upgrade();

export default upgrade;
