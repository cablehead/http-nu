// <game-board state='{"tiles":[{id,r,c,value,spawned,merged}],"ghosts":[{id,r,c,value}],"gameOver":bool}'>
//
// Fully encapsulated 4x4 board. State comes in as a JSON string on the
// `state` attribute; Datastar's `data-attr:state="JSON.stringify($sig)"`
// keeps it in sync with a signal. Each snapshot is self-describing:
//
//   tiles[].spawned   true => phase 3 spawn-in (no slide)
//   tiles[].merged    true => phase 2 pop after slide (value already
//                             reflects the doubled survivor)
//   ghosts[]          tiles consumed by merges this snapshot, with the
//                     merge-cell destination they slid into.
//
// The component reads positions for the "from" side of every slide
// directly from its internal DOM-mirror map (`this.tiles`), so there's
// no need for a prevState JSON or a diff loop. Animation: Web
// Animations API, three phases.

const STYLES = `
  :host {
    display: block;
    container-type: inline-size;
    position: relative;
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
  /* Status overlay -- up to two stacked badges pinned to the board's
     top-left. The "over" slot is neutral ("game over"); the "result"
     slot is the player's outcome (green "you win!" or red "you lost").
     The slots coexist: game-over + you-win or game-over + you-lost
     both render. Same look across every surface that embeds the
     component (/play, /watch, /my/games card, splash). */
  .badges {
    position: absolute;
    top: 0.75rem;
    left: 0.5rem;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.4rem;
    pointer-events: none;
  }
  .badge {
    padding: 0.2rem 0.7rem;
    font-size: 0.875rem;
    font-weight: 700;
    color: #fff;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    border-radius: 4px;
    box-shadow: 2px 2px 0 rgba(0, 0, 0, 0.25);
    display: none;
  }
  .badge.over.show   { display: block; background: #776e65; transform: rotate(-3deg); }
  .badge.result.won  { display: block; background: #2a9d4a; transform: rotate(-6deg); }
  .badge.result.lost { display: block; background: #e05252; transform: rotate(-3deg); }

  /* Thumbnail / "dim" variant. Used by /my/games + /by/<id> game
     cards. Everything except the highest-value tile is muted by a
     tinted overlay so the card's headline -- "how far this game
     got" -- pops without an extra max-tile badge. The board re-lights
     on hover. The max tile is z-lifted above the overlay; the WC
     tags it with .is-max on every #apply. */
  :host([dim]) .board { isolation: isolate; }
  :host([dim]) .board::after {
    content: "";
    position: absolute;
    inset: 0;
    background: rgba(0, 119, 182, 0.75);
    border-radius: inherit;
    pointer-events: none;
    z-index: 1;
    transition: opacity 120ms ease-out;
  }
  :host([dim]:hover) .board::after { opacity: 0; }
  :host([dim]) .tile.is-max { position: relative; z-index: 2; }
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
  2048: { bg: "#edc22e", fg: "#f9f6f2" },
};
// 4096+ collapses to a single dark warm near-black -- Cirulli's
// "past the end" signal: the gold ramp intentionally breaks once
// you've cleared the game's intended target.
const paletteFor = (v) => PALETTE[v] || { bg: "#3c3a32", fg: "#f9f6f2" };
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
    this.shadowRoot.innerHTML = `<style>${STYLES}</style><div class="board" part="board"></div><div class="badges"><span class="badge over"></span><span class="badge result"></span></div>`;
    this.boardEl = this.shadowRoot.querySelector(".board");
    this.overEl = this.shadowRoot.querySelector(".badge.over");
    this.resultEl = this.shadowRoot.querySelector(".badge.result");

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
    this.activeAnimations = new Set();
    this.applyToken = 0;
    this.lastAppliedJson = null;
    // Sticky once any tile has reached 2048 within this WC instance.
    // Drives the dual-badge endgame ("game over" + "you win" vs "you
    // lost") and gates the post-win hide-after-3-moves rule below.
    this.hasWon = false;
    // Count of state-changing applies observed since the win was first
    // seen. -1 before the win; 0 on the apply that crossed 2048; +1
    // per subsequent apply. The win badge stays visible while < 3, then
    // hides during continued play until the game ends.
    this.movesSinceWin = -1;
  }

  attributeChangedCallback(name, oldVal, newVal) {
    if (name !== "state") return;
    if (newVal == null) return;
    // Datastar's data-attr can write setAttribute with the unchanged
    // value after a DOM morph (e.g. /my/games re-renders the card
    // chrome around the WC; morphdom strips the runtime `state`
    // attribute that wasn't in the server-rendered HTML, then
    // Datastar's apply pass restores it). Both `oldVal === newVal`
    // and "stripped then restored" paths would otherwise cancel an
    // in-flight slide/merge by calling #apply() with no real diff.
    // Track the last-applied JSON so any duplicate is a no-op.
    if (oldVal === newVal) return;
    if (newVal === this.lastAppliedJson) return;
    let parsed;
    try { parsed = JSON.parse(newVal); }
    catch { return; }
    this.lastAppliedJson = newVal;
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

  // Tag the highest-value tile(s) with `.is-max` so :host([dim]) can
  // z-lift it above the tinted overlay. Cheap to recompute on every
  // apply; tile values change rarely (only on merges).
  #applyMaxClass(tiles) {
    if (!tiles.length) return;
    const maxV = Math.max(...tiles.map((t) => t.value));
    for (const t of tiles) {
      const entry = this.tiles.get(t.id);
      if (!entry) continue;
      entry.el.classList.toggle("is-max", t.value === maxV);
    }
  }

  // Tick the post-win move counter on every state-changing apply.
  // `attributeChangedCallback` already dedupes identical JSON, so each
  // call here is a distinct snapshot. We track sticky `hasWon` so the
  // endgame can show "you win" alongside "game over" even after the
  // win badge has been hidden during continued play.
  #tickWinCounter(state) {
    const wonNow = (state.tiles ?? []).some((t) => t.value >= 2048);
    if (wonNow && !this.hasWon) {
      this.hasWon = true;
      this.movesSinceWin = 0;
    } else if (this.hasWon) {
      this.movesSinceWin++;
    }
  }

  #applyBadge(state) {
    const over = !!state.gameOver;
    // During play: show "you win" for the winning apply and the next
    // two; hide on the third post-win apply onward. At game-over both
    // badges re-appear: "game over" + ("you win" if ever won, else
    // "you lost").
    const showOver = over;
    const showWin = over ? this.hasWon : (this.hasWon && this.movesSinceWin < 3);
    const showLost = over && !this.hasWon;

    this.overEl.classList.toggle("show", showOver);
    this.overEl.textContent = showOver ? "game over" : "";

    this.resultEl.classList.toggle("won", showWin);
    this.resultEl.classList.toggle("lost", showLost);
    this.resultEl.textContent = showWin ? "you win!" : showLost ? "you lost" : "";
  }

  async #apply(state) {
    this.#tickWinCounter(state);
    this.#applyBadge(state);
    this.#cancelActive();
    const token = ++this.applyToken;

    const tiles = state.tiles ?? [];
    const ghosts = state.ghosts ?? [];

    // Defensive: drop any DOM tile the snapshot doesn't reference.
    // Should never fire in normal operation -- tiles only leave via
    // merge (which produces a ghost). Belt-and-braces for an unusual
    // resume payload.
    const validIds = new Set();
    for (const t of tiles) validIds.add(t.id);
    for (const g of ghosts) validIds.add(g.id);
    for (const [id, entry] of this.tiles) {
      if (!validIds.has(id)) { entry.el.remove(); this.tiles.delete(id); }
    }

    // --- Phase 1: slide --------------------------------------------------
    const slideAnims = [];

    // Ghosts: existing DOM tile slides from its current cell to the
    // merge destination (carried in the ghost record), fading out.
    for (const g of ghosts) {
      const entry = this.tiles.get(g.id);
      if (!entry) continue;  // not in our DOM (e.g. first apply / replay)
      const oldR = entry.r, oldC = entry.c;
      entry.el.style.gridColumn = g.c + 1;
      entry.el.style.gridRow = g.r + 1;
      entry.r = g.r; entry.c = g.c;
      const dx = (oldC - g.c) * 100;
      const dy = (oldR - g.r) * 100;
      const a = entry.el.animate(
        [
          { transform: `translate(${dx}%, ${dy}%)`, opacity: 1 },
          { transform: "translate(0, 0)", opacity: 0 },
        ],
        { duration: SLIDE_MS, easing: "ease-out", fill: "both" },
      );
      slideAnims.push(a);
      a.addEventListener("finish", () => {
        if (this.tiles.get(g.id) === entry) {
          entry.el.remove();
          this.tiles.delete(g.id);
        }
      });
    }

    // Tiles: persisted tiles re-style + animate from current position.
    // Spawned tiles defer to phase 3. Tiles whose id isn't in our DOM
    // (first apply / replay catchup) mount in place with no animation.
    // Merge survivors carry their NEW value through the slide; the
    // post-slide pop is the merge cue and any stale value from an
    // interrupted previous apply is reset on the way in.
    for (const t of tiles) {
      if (t.spawned) continue;
      let entry = this.tiles.get(t.id);
      if (!entry) {
        const el = this.#makeTileEl(t);
        this.boardEl.appendChild(el);
        this.tiles.set(t.id, { el, r: t.r, c: t.c, value: t.value });
        continue;
      }
      const oldR = entry.r, oldC = entry.c;
      this.#styleTile(entry.el, t);
      entry.r = t.r; entry.c = t.c; entry.value = t.value;
      if (oldR === t.r && oldC === t.c) continue;
      const dx = (oldC - t.c) * 100;
      const dy = (oldR - t.r) * 100;
      const a = entry.el.animate(
        [
          { transform: `translate(${dx}%, ${dy}%)` },
          { transform: "translate(0, 0)" },
        ],
        { duration: SLIDE_MS, easing: "ease-out", fill: "both" },
      );
      slideAnims.push(a);
    }

    slideAnims.forEach((a) => this.activeAnimations.add(a));
    if (slideAnims.length) {
      await Promise.all(slideAnims.map((a) => a.finished.catch(() => {})));
      if (token !== this.applyToken) return;
    }

    // --- Phase 2: merge pop ----------------------------------------------
    const popAnims = [];
    for (const t of tiles) {
      if (!t.merged) continue;
      const entry = this.tiles.get(t.id);
      if (!entry) continue;
      popAnims.push(entry.el.animate(
        [
          { transform: "scale(1)" },
          { transform: `scale(${POP_SCALE})`, offset: 0.5 },
          { transform: "scale(1)" },
        ],
        { duration: MERGE_MS, easing: "ease-out" },
      ));
    }
    popAnims.forEach((a) => this.activeAnimations.add(a));
    if (popAnims.length) {
      await Promise.all(popAnims.map((a) => a.finished.catch(() => {})));
      if (token !== this.applyToken) return;
    }

    // --- Phase 3: spawn-in -----------------------------------------------
    const spawnAnims = [];
    for (const t of tiles) {
      if (!t.spawned) continue;
      if (this.tiles.has(t.id)) continue;  // defensive: already mounted
      const el = this.#makeTileEl(t);
      this.boardEl.appendChild(el);
      this.tiles.set(t.id, { el, r: t.r, c: t.c, value: t.value });
      spawnAnims.push(el.animate(
        [
          { transform: `scale(${SPAWN_FROM})`, opacity: 0 },
          { transform: "scale(1)", opacity: 1 },
        ],
        { duration: SPAWN_MS, easing: "ease-out", fill: "both" },
      ));
    }
    spawnAnims.forEach((a) => this.activeAnimations.add(a));
    if (spawnAnims.length) {
      await Promise.all(spawnAnims.map((a) => a.finished.catch(() => {})));
      if (token !== this.applyToken) return;
    }

    // After all phases settle, re-tag the max-value tile. Used by the
    // :host([dim]) thumbnail variant (game-card listings).
    this.#applyMaxClass(tiles);
  }
}

customElements.define("game-board", GameBoard);
