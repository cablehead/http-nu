// Input handlers for 2048. Server embeds the tab id and the /move URL as
// body data-* attributes so this file stays parameter-free and cacheable.
const tabId = document.body.dataset.tabId;
const moveUrl = document.body.dataset.moveUrl;

// End-to-end RTT: time from a move() call to the next DOM mutation in #game
// (i.e. when the SSE patch lands). We tune --decay-duration off this so the
// anticipation animation stays in motion through the network round-trip.
let pending = null;
const rtts = [];
const RTT_HISTORY = 5;
const game = document.getElementById("game");
// Always wait for the first mutation -- it's the SSE init replacing the
// server-rendered placeholder. After that, ping for an RTT seed.
let initSeen = false;

const flashRed = () => {
  document.body.classList.remove("flash-red");
  void document.body.offsetWidth;  // force reflow so animation restarts
  document.body.classList.add("flash-red");
};

const move = (intent) => {
  pending = performance.now();
  return fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tabId, intent }),
  }).then((r) => {
    if (!r.ok) { pending = null; flashRed(); }
  }).catch(() => {
    pending = null;
    flashRed();
  });
};

// body[data-conn] is driven by datastar via data-attr + data-indicator
// (see serve.nu). Watch for the down -> ok transition to trigger the pulse.
let prevConn = null;
new MutationObserver(() => {
  const conn = document.body.dataset.conn;
  if (prevConn === "down" && conn === "ok") {
    document.body.classList.remove("reconnect-pulse");
    void document.body.offsetWidth;  // force reflow so animation restarts
    document.body.classList.add("reconnect-pulse");
  }
  prevConn = conn;
}).observe(document.body, { attributes: true, attributeFilter: ["data-conn"] });

new MutationObserver(() => {
  if (!initSeen) {
    // First mutation is the SSE init render. Now that the stream's open,
    // send a no-op ping to seed the RTT estimate.
    initSeen = true;
    move("");
    return;
  }
  if (pending == null) return;
  const rtt = Math.round(performance.now() - pending);
  pending = null;
  document.querySelector("#rtt")?.replaceChildren(`${rtt}ms`);
  rtts.push(rtt);
  if (rtts.length > RTT_HISTORY) rtts.shift();
  // The spring curve peaks around 40% through the animation. To make that
  // peak land near (or just past) SSE arrival, target ~2x mean RTT so the
  // overshoot bounce is still playing when view-transition picks up.
  const mean = rtts.reduce((a, b) => a + b, 0) / rtts.length;
  const decay = Math.max(400, Math.min(1200, Math.round(mean * 2)));
  document.documentElement.style.setProperty("--decay-duration", `${decay}ms`);
}).observe(game, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-rev"] });

// Keyboard: hjkl + arrows + r-to-reset.
const keymap = {
  h: "h", ArrowLeft: "h",
  j: "j", ArrowDown: "j",
  k: "k", ArrowUp: "k",
  l: "l", ArrowRight: "l",
};
// Per-direction peak glow values for keyboard / programmatic moves. Magnitude
// is well above the alpha-saturation threshold so the edge fully lights up.
const glowFor = { h: ["--glow-x", -32], l: ["--glow-x", 32], k: ["--glow-y", -32], j: ["--glow-y", 32] };
const keyClasses = ["key-h", "key-j", "key-k", "key-l"];
addEventListener("keydown", (e) => {
  if (document.body.dataset.conn === "down") return;  // ignore input while disconnected
  if (document.getElementById("game")?.dataset.view !== "game") return;  // settings shown
  const dir = keymap[e.key];
  const intent = dir || (e.key === "r" ? "reset" : "");
  if (intent) {
    if (glowFor[intent]) {
      const [prop, val] = glowFor[intent];
      const w = document.querySelector("#board-wrap");
      w?.style.setProperty(prop, `${val}px`);
      // Synthetic anticipation: restart the directional lean animation by
      // clearing any prior key-* class, forcing a reflow, then adding the
      // new one. animationend then cleans it up.
      if (w) {
        w.classList.remove(...keyClasses);
        void w.offsetWidth;
        w.classList.add(`key-${intent}`);
        w.addEventListener("animationend", () => {
          w.classList.remove(`key-${intent}`);
        }, { once: true });
      }
    }
    // Shift + arrow auto-slides in that direction until no more movement.
    move(dir && e.shiftKey ? `shift-${dir}` : intent);
    e.preventDefault();
  }
});

// Reset button lives in the hint paragraph (static); the settings toggle
// lives inside #game and gets morphed in/out -- delegated handler catches
// both via event bubbling.
document.querySelector("p.hint button")?.addEventListener("click", () => move("reset"));

document.addEventListener("click", (e) => {
  const t = e.target.closest("[data-view-to]");
  if (!t) return;
  // Add view-flipping BEFORE the patch lands so the snapshot suppresses
  // per-tile view-transition-name. Tiles get captured as part of view-game,
  // so they flip with the board instead of fading separately.
  document.documentElement.classList.add("view-flipping");
  fetch(document.body.dataset.viewUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tabId, mode: t.dataset.viewTo }),
  });
  setTimeout(() => document.documentElement.classList.remove("view-flipping"), 600);
});


// Swipe with anticipation: while dragging, lean the tiles toward the gesture
// (CSS reads --tilt-x / --tilt-y). On release past threshold, leave the lean
// in place so the view-transition slide continues the motion. On cancel,
// snap back via the `.snap` class.
const DAMP = 0.45;
const CAP = 26;
const AXIS_LOCK = 8;  // once you move this far on one axis, the other locks
const HOLD_FOR_SHIFT_MS = 1000;  // hold after the swipe for this long -> shift
let start = null;
let wrap = null;
let axis = null;
let committedDir = null;     // direction the swipe committed to (null until threshold crossed)
let holdShiftTimer = null;   // fires shift-<dir> if the user keeps holding

addEventListener("pointerdown", (e) => {
  if (!e.target.closest("#board")) { start = null; return; }
  start = [e.clientX, e.clientY];
  axis = null;
  committedDir = null;
  if (holdShiftTimer) { clearTimeout(holdShiftTimer); holdShiftTimer = null; }
  wrap = document.querySelector("#board-wrap");
  wrap?.classList.remove("snap", "decay");
});

addEventListener("pointermove", (e) => {
  if (!start || !wrap) return;
  let dx = e.clientX - start[0];
  let dy = e.clientY - start[1];
  if (!axis && Math.max(Math.abs(dx), Math.abs(dy)) >= AXIS_LOCK) {
    axis = Math.abs(dx) > Math.abs(dy) ? "h" : "v";
  }
  if (axis === "h") dy = 0;
  if (axis === "v") dx = 0;
  const tx = Math.max(-CAP, Math.min(CAP, dx * DAMP));
  const ty = Math.max(-CAP, Math.min(CAP, dy * DAMP));
  wrap.style.setProperty("--tilt-x", `${tx}px`);
  wrap.style.setProperty("--tilt-y", `${ty}px`);
  wrap.style.setProperty("--glow-x", `${tx}px`);
  wrap.style.setProperty("--glow-y", `${ty}px`);
  // Commit on first threshold crossing -- single impulse fires immediately.
  // Then arm a hold timer: if the user keeps the finger down for
  // HOLD_FOR_SHIFT_MS, fire shift-<dir> to keep sliding until settled.
  if (!committedDir && Math.max(Math.abs(dx), Math.abs(dy)) >= 30) {
    committedDir = Math.abs(dx) > Math.abs(dy)
      ? (dx > 0 ? "l" : "h")
      : (dy > 0 ? "j" : "k");
    move(committedDir);
    holdShiftTimer = setTimeout(() => {
      holdShiftTimer = null;
      move(`shift-${committedDir}`);
    }, HOLD_FOR_SHIFT_MS);
  }
});

addEventListener("pointerup", () => {
  if (!start) return;
  start = null;
  wrap?.style.setProperty("--tilt-x", "0px");
  wrap?.style.setProperty("--tilt-y", "0px");
  if (holdShiftTimer) { clearTimeout(holdShiftTimer); holdShiftTimer = null; }
  if (committedDir) {
    // Single swipe committed during pointermove; just ease the tilt back.
    wrap?.classList.add("decay");
    setTimeout(() => wrap?.classList.remove("decay"), 1300);
  } else {
    // Below threshold: cancel, spring tilt + glow back to rest.
    wrap?.style.setProperty("--glow-x", "0px");
    wrap?.style.setProperty("--glow-y", "0px");
    wrap?.classList.add("snap");
    setTimeout(() => wrap?.classList.remove("snap"), 260);
  }
  committedDir = null;
});
