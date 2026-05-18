// <game-board state='{"tiles":[{id,r,c,value},...]}'>
//
// Fully encapsulated 4x4 board. State comes in as a JSON string on the
// `state` attribute; Datastar's `data-attr-state="JSON.stringify($sig)"`
// keeps it in sync with a signal. The component diffs new state against
// the previous one (tile ids are stable across snapshots) and plays a
// 3-phase animation: slide -> merge-pop -> spawn-in. Web Animations API,
// no view-transition.
//
// The snapshot may carry `spawned` / `merged` / `ghosts` fields from the
// server-side renderer -- we ignore them. Diff-by-id is the source of
// truth for what slides, what pops, and what spawns.

const STYLES = `
  :host {
    display: block;
    container-type: inline-size;
  }
  .board {
    position: relative;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    grid-template-rows: repeat(4, 1fr);
    aspect-ratio: 1;
    gap: 6px;
    padding: 6px;
    background: #bbada0;
    border-radius: 6px;
  }
  .cell {
    background: rgba(238, 228, 218, 0.35);
    border-radius: 3px;
  }
  .tile {
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    font-family: "Source Sans 3", system-ui, sans-serif;
    font-weight: 700;
    will-change: transform, opacity;
  }
`;

const PALETTE = {
  2:    { bg: "#eee4da", fg: "#776e65" },
  4:    { bg: "#ede0c8", fg: "#776e65" },
  8:    { bg: "#f2b179", fg: "#f9f6f2" },
  16:   { bg: "#f59563", fg: "#f9f6f2" },
  32:   { bg: "#f67c5f", fg: "#f9f6f2" },
  64:   { bg: "#f65e3b", fg: "#f9f6f2" },
  128:  { bg: "#edcf72", fg: "#f9f6f2" },
  256:  { bg: "#edcc61", fg: "#f9f6f2" },
  512:  { bg: "#edc850", fg: "#f9f6f2" },
  1024: { bg: "#edc53f", fg: "#f9f6f2" },
};
const paletteFor = (v) => PALETTE[v] || { bg: "#edc22e", fg: "#f9f6f2" };
const fontSizeCqw = (v) => (v >= 1024 ? 5 : v >= 128 ? 6 : 7);

const SLIDE_MS = 180;
const MERGE_MS = 140;
const SPAWN_MS = 140;
const POP_SCALE = 1.18;
const SPAWN_FROM = 0.4;

class GameBoard extends HTMLElement {
  static get observedAttributes() { return ["state"]; }

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.shadowRoot.innerHTML = `<style>${STYLES}</style><div class="board" part="board"></div>`;
    this.boardEl = this.shadowRoot.querySelector(".board");

    for (let r = 0; r < 4; r++) {
      for (let c = 0; c < 4; c++) {
        const cell = document.createElement("div");
        cell.className = "cell";
        cell.style.gridColumn = c + 1;
        cell.style.gridRow = r + 1;
        this.boardEl.appendChild(cell);
      }
    }

    this.tiles = new Map();
    this.prevState = null;
    this.activeAnimations = new Set();
    this.applyToken = 0;
  }

  attributeChangedCallback(name, _oldVal, newVal) {
    if (name !== "state" || newVal == null) return;
    let parsed;
    try { parsed = JSON.parse(newVal); }
    catch { return; }
    this.#apply(parsed);
  }

  #cancelActive() {
    this.activeAnimations.forEach((a) => a.cancel());
    this.activeAnimations.clear();
  }

  #styleTile(el, t) {
    const p = paletteFor(t.value);
    el.style.gridColumn = t.c + 1;
    el.style.gridRow = t.r + 1;
    el.style.background = p.bg;
    el.style.color = p.fg;
    el.style.fontSize = `${fontSizeCqw(t.value)}cqw`;
    el.textContent = String(t.value);
  }

  #makeTileEl(t) {
    const el = document.createElement("div");
    el.className = "tile";
    this.#styleTile(el, t);
    return el;
  }

  async #apply(newState) {
    this.#cancelActive();
    const token = ++this.applyToken;
    const oldState = this.prevState ?? { tiles: [] };
    this.prevState = newState;

    const oldTiles = oldState.tiles ?? [];
    const newTiles = newState.tiles ?? [];
    const oldById = new Map(oldTiles.map((t) => [t.id, t]));
    const newById = new Map(newTiles.map((t) => [t.id, t]));

    const persisted = newTiles.filter((t) => oldById.has(t.id));
    const spawned = newTiles.filter((t) => !oldById.has(t.id));
    const mergedSurvivors = persisted.filter(
      (t) => oldById.get(t.id).value !== t.value,
    );
    const consumedIds = [...oldById.keys()].filter((id) => !newById.has(id));

    // For each consumed tile, find the survivor it merged into. The
    // survivor's value is 2x the consumed tile's, and in the OLD state
    // the survivor sat in the same row or column (slide is axis-aligned).
    // Pick the closest such survivor.
    const consumedTarget = new Map();
    for (const cid of consumedIds) {
      const c = oldById.get(cid);
      let best = null;
      let bestDist = Infinity;
      for (const m of mergedSurvivors) {
        if (m.value !== c.value * 2) continue;
        const mOld = oldById.get(m.id);
        const sameRow = mOld.r === c.r;
        const sameCol = mOld.c === c.c;
        if (!sameRow && !sameCol) continue;
        const dist = sameRow ? Math.abs(mOld.c - c.c) : Math.abs(mOld.r - c.r);
        if (dist < bestDist) { bestDist = dist; best = m; }
      }
      if (best) consumedTarget.set(cid, best);
    }

    // Remove any orphan tiles in our DOM that aren't in old or new state
    // (defensive -- only fires if the previous animation was interrupted).
    for (const [id, entry] of this.tiles) {
      if (!newById.has(id) && !oldById.has(id)) {
        entry.el.remove();
        this.tiles.delete(id);
      }
    }

    // --- Phase 1: slide ---------------------------------------------------
    const slideAnims = [];

    for (const t of persisted) {
      const old = oldById.get(t.id);
      let entry = this.tiles.get(t.id);
      if (!entry) {
        // We weren't tracking this id yet (e.g. first apply after a SSE
        // resume with no prevState). Mount it at its old position so the
        // slide has somewhere to start from.
        const el = this.#makeTileEl(old);
        this.boardEl.appendChild(el);
        entry = { el, ...old };
        this.tiles.set(t.id, entry);
      }
      entry.el.style.gridColumn = t.c + 1;
      entry.el.style.gridRow = t.r + 1;
      const dx = (old.c - t.c) * 100;
      const dy = (old.r - t.r) * 100;
      if (dx || dy) {
        const a = entry.el.animate(
          [
            { transform: `translate(${dx}%, ${dy}%)` },
            { transform: "translate(0, 0)" },
          ],
          { duration: SLIDE_MS, easing: "ease-out", fill: "both" },
        );
        slideAnims.push(a);
      }
      entry.r = t.r;
      entry.c = t.c;
    }

    for (const cid of consumedIds) {
      const entry = this.tiles.get(cid);
      if (!entry) continue;
      const target = consumedTarget.get(cid);
      let a;
      if (target) {
        const oldR = entry.r, oldC = entry.c;
        entry.el.style.gridColumn = target.c + 1;
        entry.el.style.gridRow = target.r + 1;
        const dx = (oldC - target.c) * 100;
        const dy = (oldR - target.r) * 100;
        a = entry.el.animate(
          [
            { transform: `translate(${dx}%, ${dy}%)`, opacity: 1 },
            { transform: "translate(0, 0)", opacity: 0 },
          ],
          { duration: SLIDE_MS, easing: "ease-out", fill: "both" },
        );
      } else {
        // No merge target -- tile is vanishing without a destination
        // (only happens in the design playground when ids don't carry
        // forward between curated states). Fade in place.
        a = entry.el.animate(
          [{ opacity: 1 }, { opacity: 0 }],
          { duration: SLIDE_MS, easing: "ease-out", fill: "both" },
        );
      }
      slideAnims.push(a);
      a.addEventListener("finish", () => {
        if (this.tiles.get(cid) === entry) {
          entry.el.remove();
          this.tiles.delete(cid);
        }
      });
    }

    slideAnims.forEach((a) => this.activeAnimations.add(a));
    if (slideAnims.length) {
      await Promise.all(slideAnims.map((a) => a.finished.catch(() => {})));
      if (token !== this.applyToken) return;
    }

    // Bump merge-survivor values AFTER the slide so the doubled number
    // doesn't appear on the still-sliding survivor.
    for (const t of mergedSurvivors) {
      const entry = this.tiles.get(t.id);
      if (!entry) continue;
      this.#styleTile(entry.el, t);
    }

    // --- Phase 2: merge pop ----------------------------------------------
    const popAnims = mergedSurvivors.map((t) => {
      const entry = this.tiles.get(t.id);
      return entry.el.animate(
        [
          { transform: "scale(1)" },
          { transform: `scale(${POP_SCALE})`, offset: 0.5 },
          { transform: "scale(1)" },
        ],
        { duration: MERGE_MS, easing: "ease-out" },
      );
    });
    popAnims.forEach((a) => this.activeAnimations.add(a));
    if (popAnims.length) {
      await Promise.all(popAnims.map((a) => a.finished.catch(() => {})));
      if (token !== this.applyToken) return;
    }

    // --- Phase 3: spawn-in -----------------------------------------------
    const spawnAnims = spawned.map((t) => {
      const el = this.#makeTileEl(t);
      this.boardEl.appendChild(el);
      this.tiles.set(t.id, { el, ...t });
      return el.animate(
        [
          { transform: `scale(${SPAWN_FROM})`, opacity: 0 },
          { transform: "scale(1)", opacity: 1 },
        ],
        { duration: SPAWN_MS, easing: "ease-out", fill: "both" },
      );
    });
    spawnAnims.forEach((a) => this.activeAnimations.add(a));
    if (spawnAnims.length) {
      await Promise.all(spawnAnims.map((a) => a.finished.catch(() => {})));
    }
  }
}

customElements.define("game-board", GameBoard);
