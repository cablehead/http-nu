/**
 * <mermaid-diagram> web component
 *
 * Renders Mermaid diagrams from text content. Reacts to DOM mutations
 * (morph-friendly) and attribute changes. Shadow DOM keeps SVG output
 * separate from the source text so DOM morphers (Datastar, idiomorph)
 * can patch the source without colliding with rendered output.
 *
 * Usage:
 *   <mermaid-diagram>
 *     graph TD
 *       A --> B
 *   </mermaid-diagram>
 *
 *   <mermaid-diagram theme="dark">
 *     sequenceDiagram
 *       Alice->>Bob: Hello
 *   </mermaid-diagram>
 *
 * With Datastar:
 *   <mermaid-diagram data-attr-theme="$darkMode ? 'dark' : 'default'">
 *     graph TD; A --> B
 *   </mermaid-diagram>
 *
 * Attributes:
 *   theme  - Mermaid theme (default | dark | forest | neutral)
 *
 * Events:
 *   rendered - { detail: { svg: string } }
 *   error    - { detail: { message: string } }
 *
 * Configuration:
 *   MermaidDiagram.src    - ESM import URL (default: jsdelivr CDN)
 *   MermaidDiagram.config - Extra mermaid.initialize() options
 */

class MermaidDiagram extends HTMLElement {
  static observedAttributes = ['theme'];

  /** Override to change the mermaid ESM source */
  static src = 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';

  /** Extra options merged into mermaid.initialize() */
  static config = {};

  static #mermaid;
  static #loading;
  static #queue = Promise.resolve();

  #container;
  #observer;
  #timer;
  #connected = false;

  constructor() {
    super();
    const shadow = this.attachShadow({ mode: 'open' });
    shadow.innerHTML = '<style>:host{display:block}</style><div></div>';
    this.#container = shadow.querySelector('div');
  }

  connectedCallback() {
    this.#connected = true;
    this.#observer = new MutationObserver(() => this.#schedule());
    this.#observer.observe(this, {
      childList: true,
      characterData: true,
      subtree: true,
    });
    this.#schedule();
  }

  disconnectedCallback() {
    this.#connected = false;
    this.#observer?.disconnect();
    clearTimeout(this.#timer);
  }

  attributeChangedCallback(_name, old, val) {
    if (old !== val) this.#schedule();
  }

  #schedule() {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      MermaidDiagram.#queue = MermaidDiagram.#queue.then(() => this.#render());
    }, 10);
  }

  async #render() {
    if (!this.#connected) return;

    const source = this.textContent?.trim();
    if (!source) {
      this.#container.innerHTML = '';
      return;
    }

    try {
      const mermaid = await MermaidDiagram.#load();
      const theme = this.getAttribute('theme') || 'default';
      mermaid.initialize({
        startOnLoad: false,
        theme,
        ...MermaidDiagram.config,
      });

      const id = `md-${crypto.randomUUID().slice(0, 8)}`;
      const { svg } = await mermaid.render(id, source);

      if (!this.#connected) return;
      this.#container.innerHTML = svg;

      this.dispatchEvent(
        new CustomEvent('rendered', {
          detail: { svg },
          bubbles: true,
          composed: true,
        }),
      );
    } catch (err) {
      if (!this.#connected) return;

      const msg = err.message || 'Render failed';
      this.#container.innerHTML = `<pre style="color:red;font-size:14px;margin:0">${msg.replace(/</g, '&lt;')}</pre>`;

      this.dispatchEvent(
        new CustomEvent('error', {
          detail: { message: msg },
          bubbles: true,
          composed: true,
        }),
      );
    }
  }

  static async #load() {
    if (this.#mermaid) return this.#mermaid;

    // Prefer globally loaded mermaid (script tag)
    if (window.mermaid) {
      this.#mermaid = window.mermaid;
      return this.#mermaid;
    }

    // Lazy ESM import
    if (!this.#loading) {
      this.#loading = import(this.src).then((m) => {
        this.#mermaid = m.default || m;
        return this.#mermaid;
      });
    }

    return this.#loading;
  }
}

customElements.define('mermaid-diagram', MermaidDiagram);

export default MermaidDiagram;
