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
// If #game already has children when this script runs, SSE init beat us here.
let initSeen = game.childElementCount > 0;

const move = (intent) => {
  pending = performance.now();
  return fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tabId, intent }),
  });
};

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
  const mean = rtts.reduce((a, b) => a + b, 0) / rtts.length;
  const decay = Math.max(350, Math.min(900, Math.round(mean) + 150));
  document.documentElement.style.setProperty("--decay-duration", `${decay}ms`);
}).observe(game, { childList: true, subtree: true });

// In case SSE init beat the observer setup, ping immediately.
if (initSeen) move("");

// Keyboard: hjkl + arrows + r-to-reset.
const keymap = {
  h: "h", ArrowLeft: "h",
  j: "j", ArrowDown: "j",
  k: "k", ArrowUp: "k",
  l: "l", ArrowRight: "l",
};
addEventListener("keydown", (e) => {
  const intent = keymap[e.key] || (e.key === "r" ? "reset" : "");
  if (intent) {
    move(intent);
    e.preventDefault();
  }
});

// Reset button (visible tap target for touch users).
document.querySelector("button")?.addEventListener("click", () => move("reset"));

// Live glow-color picker -- writes to the --glow CSS variable and echoes
// the hex into #glow-hex so it can be read back / shared.
document.getElementById("glow")?.addEventListener("input", (e) => {
  document.documentElement.style.setProperty("--glow", e.target.value);
  const out = document.getElementById("glow-hex");
  if (out) out.textContent = e.target.value;
});

// Swipe with anticipation: while dragging, lean the tiles toward the gesture
// (CSS reads --tilt-x / --tilt-y). On release past threshold, leave the lean
// in place so the view-transition slide continues the motion. On cancel,
// snap back via the `.snap` class.
const DAMP = 0.45;
const CAP = 26;
const AXIS_LOCK = 8;  // once you move this far on one axis, the other locks
let start = null;
let wrap = null;
let axis = null;
addEventListener("pointerdown", (e) => {
  if (!e.target.closest("#board")) { start = null; return; }
  start = [e.clientX, e.clientY];
  axis = null;
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
});
addEventListener("pointerup", (e) => {
  if (!start) return;
  const dx = e.clientX - start[0];
  const dy = e.clientY - start[1];
  start = null;
  wrap?.style.setProperty("--tilt-x", "0px");
  wrap?.style.setProperty("--tilt-y", "0px");
  if (Math.max(Math.abs(dx), Math.abs(dy)) >= 30) {
    // Commit: tiles ease through zero with overshoot, but --glow-x/--glow-y
    // are left at peak -- the edge stays lit until the SSE patch arrives and
    // the view-transition cross-fade carries it away.
    wrap?.classList.add("decay");
    setTimeout(() => wrap?.classList.remove("decay"), 1000);
    move(
      Math.abs(dx) > Math.abs(dy)
        ? dx > 0 ? "l" : "h"
        : dy > 0 ? "j" : "k",
    );
  } else {
    // Cancel: tilt and glow both spring back together.
    wrap?.style.setProperty("--glow-x", "0px");
    wrap?.style.setProperty("--glow-y", "0px");
    wrap?.classList.add("snap");
    setTimeout(() => wrap?.classList.remove("snap"), 260);
  }
});
