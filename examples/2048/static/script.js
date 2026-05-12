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

// Swipe: stash start coords on pointerdown if it begins inside the board,
// pick the dominant axis on pointerup past a 30px threshold.
let start = null;
addEventListener("pointerdown", (e) => {
  start = e.target.closest("#board") ? [e.clientX, e.clientY] : null;
});
addEventListener("pointerup", (e) => {
  if (!start) return;
  const dx = e.clientX - start[0];
  const dy = e.clientY - start[1];
  if (Math.max(Math.abs(dx), Math.abs(dy)) < 30) return;
  move(
    Math.abs(dx) > Math.abs(dy)
      ? dx > 0 ? "l" : "h"
      : dy > 0 ? "j" : "k",
  );
});
