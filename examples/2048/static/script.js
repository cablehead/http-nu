// Input handlers for 2048. Server embeds the player id and the /move URL
// as body data-* attributes so this file stays parameter-free and cacheable.
const playerId = document.body.dataset.playerId;
const gameId = document.body.dataset.gameId;
const moveUrl = document.body.dataset.moveUrl;
const homeHref = document.body.dataset.homeHref;
const newHref = document.body.dataset.newHref;

// End-to-end RTT: time from a move() call to the next DOM mutation in
// #game (i.e. when the SSE patch lands). Drives the #rtt readout.
let pending = null;
const game = document.getElementById("game");
// Always wait for the first mutation -- it's the SSE init replacing the
// server-rendered placeholder. After that, ping for an RTT seed.
let initSeen = false;

const flashRed = () => {
  document.body.classList.remove("flash-red");
  void document.body.offsetWidth;  // force reflow so animation restarts
  document.body.classList.add("flash-red");
};

const rttEl = () => document.querySelector("#rtt");
const tickRtt = () => {
  if (pending == null) return;
  rttEl()?.replaceChildren(`${Math.round(performance.now() - pending.t)}ms`);
  requestAnimationFrame(tickRtt);
};

const move = (intent) => {
  // Each move carries a uuid the server echoes back via #game's data-rev.
  // The observer only counts an RTT when data-rev matches our pending id,
  // so reconnect-replay patches don't get misattributed to a probe.
  const reqId = crypto.randomUUID();
  pending = { id: reqId, t: performance.now() };
  // Light the directional edge-line indicator for the round trip.
  // Cleared in the MutationObserver below when the SSE patch lands.
  if (intent && "hjkl".includes(intent)) {
    document.querySelector("#board-wrap")?.setAttribute("data-pending", intent);
  }
  requestAnimationFrame(tickRtt);  // live-tick the RTT indicator while in flight
  return fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ playerId, gameId, intent, reqId }),
  }).then((r) => {
    if (!r.ok) {
      pending = null;
      document.querySelector("#board-wrap")?.removeAttribute("data-pending");
      flashRed();
    }
  }).catch(() => {
    pending = null;
    document.querySelector("#board-wrap")?.removeAttribute("data-pending");
    flashRed();
  });
};

// SSE liveness signal. Server interleaves a no-op datastar-patch-signals
// every 450ms; we watch datastar-fetch CustomEvents for our SSE element
// only, refresh a staleness timer on any sign-of-life event, and flip to
// "down" if 1000ms passes with no refresh (~2 missed heartbeats).
// We ignore datastar's $connected / data-indicator because it stays true
// through retry loops -- not a real liveness signal.
const SSE_STALE_MS = 1000;
const sseEl = document.querySelector("[data-sse]");
let prevConn = null;
let staleTimer = null;
const setConn = (v) => {
  if (document.body.dataset.conn === v) return;
  document.body.dataset.conn = v;
  if (v === "down") {
    // Old RTT readings are stale across a disconnect; clear the
    // indicator and the history so reconnect probes for a fresh
    // measurement.
    pending = null;
    rttEl()?.replaceChildren("");
  }
  if (prevConn === "down" && v === "ok" && moveUrl) {
    // Re-probe RTT after reconnect. (Only on /play -- /games has no move URL.)
    move("");
  }
  prevConn = v;
};
const sseAlive = () => {
  setConn("ok");
  clearTimeout(staleTimer);
  staleTimer = setTimeout(() => setConn("down"), SSE_STALE_MS);
};
document.addEventListener("datastar-fetch", (e) => {
  if (e.detail.el !== sseEl) return;  // not our SSE -- ignore
  const t = e.detail.type;
  // Any successful event from our SSE is a sign of life: lifecycle
  // 'started', or any 'datastar-patch-*' message (including heartbeats).
  if (t === "started" || t.startsWith("datastar-patch")) sseAlive();
  if (t === "retrying" || t === "retries-failed") setConn("down");
});

// MutationObserver, keyboard, and pointer handlers below are /play-only --
// they all need the #game element to exist. Guard so script.js can also
// be loaded on /games (where we just want the SSE connection tracker
// above to update #conn).
if (game) {
// Move acks ride the $lastReqId signal, fired from the SSE pipeline as
// soon as it sees the move frame. Called via data-effect on the hidden
// element in the /play body. No-op until reqId matches a pending probe
// we issued (so the initial mount call, replay patches, and spectator
// streams all just fall through). #rtt stays blank until the user
// makes their first move -- that move populates it.
window.onAck = (reqId) => {
  if (pending == null || reqId !== pending.id) return;
  const rtt = Math.round(performance.now() - pending.t);
  pending = null;
  document.querySelector("#board-wrap")?.removeAttribute("data-pending");
  document.querySelector("#rtt")?.replaceChildren(`${rtt}ms`);
};

// Keyboard: hjkl + arrows + u-to-undo. (New game lives on the splash;
// reset key is intentionally gone.)
const keymap = {
  h: "h", ArrowLeft: "h",
  j: "j", ArrowDown: "j",
  k: "k", ArrowUp: "k",
  l: "l", ArrowRight: "l",
};
// Global navigation: Esc -> splash, n -> new game. Registered always so
// every page (play, watch, my games, splash, notes, design) responds.
addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    location.href = homeHref;
    e.preventDefault();
    return;
  }
  if (e.key === "n" && !e.ctrlKey && !e.metaKey && !e.altKey) {
    location.href = newHref;
    e.preventDefault();
  }
});

// Move impulses (h/j/k/l/u). Registered ONLY on the owner's /play page;
// spectator /watch and chrome pages don't bind this handler at all, so
// keystrokes never reach a move() call from outside the editor.
if (document.body.classList.contains("play")) {
  addEventListener("keydown", (e) => {
    if (document.body.dataset.conn === "down") return;
    // Shift+letter sends uppercase ("H"); fall back to lowercased key.
    const dir = keymap[e.key] || keymap[(e.key + "").toLowerCase()];
    const intent = dir || (e.key === "u" ? "undo" : "");
    if (intent) {
      move(intent);
      e.preventDefault();
    }
  });
}

// Delegated click handler for kbd-btn move triggers. Game-move kbd-btns
// render as <button data-intent="h"|"undo"|...>; nav kbd-btns render as
// <a href> (native navigation, right-click-open-tab works).
document.addEventListener("click", (e) => {
  const intent = e.target.closest("button[data-intent]");
  if (intent) move(intent.dataset.intent);
});



// Pointer swipe: detect a directional gesture on the board and dispatch
// a move on release. No tilt / glow during drag; the edge-line pending
// indicator is wired through move() -> #board-wrap[data-pending].
const SWIPE_THRESHOLD = 30;
let swipeStart = null;
addEventListener("pointerdown", (e) => {
  swipeStart = e.target.closest(".board") ? [e.clientX, e.clientY] : null;
});
addEventListener("pointerup", (e) => {
  if (!swipeStart) return;
  const dx = e.clientX - swipeStart[0];
  const dy = e.clientY - swipeStart[1];
  swipeStart = null;
  if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE_THRESHOLD) return;
  const dir = Math.abs(dx) > Math.abs(dy)
    ? (dx > 0 ? "l" : "h")
    : (dy > 0 ? "j" : "k");
  move(dir);
});

}

// Keep splash card "last played" labels current without a server round
// trip. Each .overlay.active span carries data-played-ms = the unix
// millisecond timestamp from the game's last-move id; we recompute the
// relative string every few seconds. Same shape as the server's
// last-active-from-id in render.nu.
function relativeFromMs(ms) {
  const diff = Math.floor((Date.now() - ms) / 1000);
  if (diff < 60) return "in play";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
  return `${Math.floor(diff / 604800)}w ago`;
}
function updateActiveLabels() {
  document.querySelectorAll(".overlay.active[data-played-ms]").forEach((el) => {
    const ms = parseInt(el.dataset.playedMs, 10);
    if (!Number.isFinite(ms)) return;
    const next = relativeFromMs(ms);
    if (el.textContent !== next) el.textContent = next;
  });
}
// Tick every 5s: fast enough to catch "in play" -> "1m ago" near the
// minute boundary, slow enough to be cheap.
setInterval(updateActiveLabels, 5000);
updateActiveLabels();


// Splash audio toggle: [ p ] kbd-btn next to the splash credit. Click
// the button or press the "p" key to toggle play/pause. aria-pressed
// reflects state so CSS can style the kbd-btn while playing.
const audioToggle = document.querySelector(".audio-toggle");
const splashAudio = document.querySelector("#splash-audio");
if (audioToggle && splashAudio) {
  let seeded = false;
  const toggleAudio = (e) => {
    if (e) e.preventDefault();
    if (splashAudio.paused) {
      // Seed first play 48s in -- past the long ambient intro on the
      // mobygratis track. Subsequent toggles keep their position.
      if (!seeded) {
        splashAudio.currentTime = 48;
        seeded = true;
      }
      splashAudio.play();
    } else {
      splashAudio.pause();
    }
  };
  audioToggle.addEventListener("click", toggleAudio);
  const sync = () => {
    const playing = !splashAudio.paused;
    audioToggle.setAttribute("aria-pressed", playing ? "true" : "false");
    audioToggle.setAttribute("aria-label", playing ? "pause audio" : "play audio");
  };
  sync();
  splashAudio.addEventListener("play", sync);
  splashAudio.addEventListener("pause", sync);
  splashAudio.addEventListener("ended", sync);
  document.addEventListener("keydown", (e) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.key === "p" || e.key === "P") {
      e.preventDefault();
      toggleAudio();
    }
  });
}
