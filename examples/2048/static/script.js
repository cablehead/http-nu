// Input handlers for 2048. Server embeds the tab id and the /move URL as
// body data-* attributes so this file stays parameter-free and cacheable.
const tabId = document.body.dataset.tabId;
const moveUrl = document.body.dataset.moveUrl;

const move = (intent) =>
  fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tabId, intent }),
  });

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

// Swipe with anticipation: while dragging, lean the tiles toward the gesture
// (CSS reads --tilt-x / --tilt-y). On release past threshold, leave the lean
// in place so the view-transition slide continues the motion. On cancel,
// snap back via the `.snap` class.
const DAMP = 0.35;
const CAP = 16;
let start = null;
let board = null;
addEventListener("pointerdown", (e) => {
  if (!e.target.closest("#board")) { start = null; return; }
  start = [e.clientX, e.clientY];
  board = document.querySelector("#board");
  board?.classList.remove("snap");
});
addEventListener("pointermove", (e) => {
  if (!start || !board) return;
  const dx = Math.max(-CAP, Math.min(CAP, (e.clientX - start[0]) * DAMP));
  const dy = Math.max(-CAP, Math.min(CAP, (e.clientY - start[1]) * DAMP));
  board.style.setProperty("--tilt-x", `${dx}px`);
  board.style.setProperty("--tilt-y", `${dy}px`);
});
addEventListener("pointerup", (e) => {
  if (!start) return;
  const dx = e.clientX - start[0];
  const dy = e.clientY - start[1];
  start = null;
  if (Math.max(Math.abs(dx), Math.abs(dy)) >= 30) {
    move(
      Math.abs(dx) > Math.abs(dy)
        ? dx > 0 ? "l" : "h"
        : dy > 0 ? "j" : "k",
    );
  } else {
    board?.classList.add("snap");
    board?.style.setProperty("--tilt-x", "0px");
    board?.style.setProperty("--tilt-y", "0px");
    setTimeout(() => board?.classList.remove("snap"), 200);
  }
});
