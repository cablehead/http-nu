// <scrub-knob value="<int>" max="<int>"> -- fine-grained scrubber.
//
// Native <input type="range"> maps cursor x to value linearly: 1873
// frames across a narrow row gives ~8 frames per pixel, way too coarse
// for selecting individual frames. This WC decouples pixel travel from
// value travel: on pointer-down it grabs pointer-capture (cursor stays
// visible), accumulates `clientX` deltas, and advances the value one
// frame per N pixels (tunable -- see PX_PER_FRAME_DEFAULT below).
// Emits `scrub` on every integer step and `scrub-end` on release.
// Keyboard arrows move one frame; PageUp/PageDown move ten; Home/End
// jump to bounds.
//
// We previously used pointer-lock (DAW-style infinite drag) but the
// browser-mandated "your pointer is hidden, press esc" banner is too
// intrusive in Safari. Capture trades infinite drag range for visible
// cursor + no banner -- long scrubs need a lift-and-redrag.
//
// Signal-in: caller sets `value` and `max` attributes (Datastar's
// `data-attr:value="$pos"` works). Signal-out: caller listens for the
// `scrub` (continuous) and `scrub-end` (pointer release) events and
// updates the signal in the handler.

// Pixels of mouse travel per one-frame step. Lower = more sensitive.
// Native <input type="range"> on this splash sits at ~0.13 px/frame
// (1873 frames across a ~240 px row). Without pointer-lock the drag
// distance is bounded by the slider width, so the default has to give
// a useful range in one pass: 0.5 px/frame gives ~480 frames per
// slider-width drag, ~4x less sensitive than native. Tune per-host
// via the `px-per-frame` attribute.
const PX_PER_FRAME_DEFAULT = 0.5;

const STYLES = `
  :host {
    display: block;
    position: relative;
    height: 24px;
    cursor: ew-resize;
    user-select: none;
    outline: none;
  }
  :host(:focus-visible) .track { outline: 2px solid var(--accent, dodgerblue); outline-offset: 3px; }
  .track {
    position: absolute;
    inset: 50% 0 auto 0;
    transform: translateY(-50%);
    height: 4px;
    background: rgba(255, 255, 255, 0.2);
    border-radius: 2px;
  }
  .fill {
    position: absolute;
    top: 0; left: 0; bottom: 0;
    background: var(--accent, dodgerblue);
    border-radius: inherit;
  }
  .thumb {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin-left: -7px;
    background: var(--accent, dodgerblue);
    border-radius: 50%;
    transform: translateY(-50%);
    pointer-events: none;
  }
`;

class ScrubKnob extends HTMLElement {
  static get observedAttributes() { return ["value", "max", "px-per-frame"]; }

  #v = 0;
  #max = 0;
  #acc = 0;
  #dragId = null;
  #lastClientX = 0;
  #pxPerFrame = PX_PER_FRAME_DEFAULT;

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.shadowRoot.innerHTML = `<style>${STYLES}</style><div class="track"><div class="fill"></div></div><div class="thumb"></div>`;
    this.fillEl = this.shadowRoot.querySelector(".fill");
    this.thumbEl = this.shadowRoot.querySelector(".thumb");
  }

  connectedCallback() {
    if (!this.hasAttribute("tabindex")) this.tabIndex = 0;
    if (!this.hasAttribute("role")) this.setAttribute("role", "slider");
    this.addEventListener("pointerdown", this.#onPointerDown);
    this.addEventListener("pointermove", this.#onPointerMove);
    this.addEventListener("pointerup", this.#onPointerUp);
    this.addEventListener("pointercancel", this.#onPointerUp);
    this.addEventListener("keydown", this.#onKeyDown);
  }

  attributeChangedCallback(name, _old, val) {
    if (val == null) return;
    // Datastar's data-attr writes JSON.stringify($sig) into the
    // attribute, which yields the literal string "null" for NaN
    // signals (JSON has no NaN). Bail on anything not finite so a
    // transiently bad upstream value can't poison internal state.
    const n = +val;
    if (!Number.isFinite(n)) return;
    if (name === "value") this.#v = n;
    else if (name === "max") this.#max = n;
    else if (name === "px-per-frame") this.#pxPerFrame = n > 0 ? n : PX_PER_FRAME_DEFAULT;
    this.#paint();
  }

  // movementX on PointerEvent isn't portable -- Safari only populates
  // it during pointer-lock, and touch/pen pointers omit it too. We
  // diff clientX against the previous pointer position ourselves,
  // seeded on pointerdown.
  #onPointerDown = (e) => {
    e.preventDefault();
    // Don't auto-focus on mouse-down: Safari shows a focus ring on
    // click-then-focus that reads as "selected" even though
    // :focus-visible is supposed to suppress it. Keyboard users still
    // get focus via Tab.
    this.#acc = 0;
    this.#dragId = e.pointerId;
    this.#lastClientX = e.clientX;
    this.setPointerCapture(e.pointerId);
  };

  #onPointerMove = (e) => {
    if (e.pointerId !== this.#dragId) return;
    const dx = e.clientX - this.#lastClientX;
    this.#lastClientX = e.clientX;
    this.#acc += dx;
    const d = Math.trunc(this.#acc / this.#pxPerFrame);
    if (!d) return;
    this.#acc -= d * this.#pxPerFrame;
    this.#commit(this.#v + d, "scrub");
  };

  #onPointerUp = (e) => {
    if (e.pointerId !== this.#dragId) return;
    this.releasePointerCapture(e.pointerId);
    this.#dragId = null;
    this.dispatchEvent(new CustomEvent("scrub-end", { detail: { value: this.#v } }));
  };

  #onKeyDown = (e) => {
    let step = 0;
    switch (e.key) {
      case "ArrowLeft":  step = -1; break;
      case "ArrowRight": step =  1; break;
      case "PageUp":     step = -10; break;
      case "PageDown":   step =  10; break;
      case "Home":       e.preventDefault(); return this.#commit(0, "scrub");
      case "End":        e.preventDefault(); return this.#commit(this.#max, "scrub");
      default: return;
    }
    e.preventDefault();
    this.#commit(this.#v + step, "scrub");
  };

  // Apply a new value (clamped) and notify. Skipping when the clamped
  // value is unchanged keeps the event stream tight -- a no-op drag tick
  // or holding an arrow at the end of the range produces no traffic.
  #commit(v, eventName) {
    const clamped = Math.max(0, Math.min(this.#max, v));
    if (clamped === this.#v) return;
    this.#v = clamped;
    this.#paint();
    this.dispatchEvent(new CustomEvent(eventName, { detail: { value: clamped } }));
  }

  #paint() {
    if (!this.#max) return;
    const pct = (this.#v / this.#max) * 100;
    this.fillEl.style.width = `${pct}%`;
    this.thumbEl.style.left = `${pct}%`;
    this.setAttribute("aria-valuenow", String(this.#v));
    this.setAttribute("aria-valuemax", String(this.#max));
    this.setAttribute("aria-valuemin", "0");
  }
}

customElements.define("scrub-knob", ScrubKnob);
