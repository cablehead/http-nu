// Input handlers for 2048. Server embeds the player id and the /move URL
// as body data-* attributes so this file stays parameter-free and cacheable.
const playerId = document.body.dataset.playerId;
const gameId = document.body.dataset.gameId;
const moveUrl = document.body.dataset.moveUrl;
const homeHref = document.body.dataset.homeHref;
const newHref = document.body.dataset.newHref;
const pingUrl = document.body.dataset.pingUrl;
// Per-page scope advertised to /presence/ping. Pulled from body
// data-scope when the route sets it explicitly; otherwise derived
// from the URL's first path segment after stripping the mount prefix
// (e.g. "/2048/play/abc" with mount "/2048" -> scope "play"). Root
// path falls back to "splash". gameId is reused for /play + /watch.
const mountPrefix = document.body.dataset.mountPrefix || "";
const scope = (() => {
  if (document.body.dataset.scope) return document.body.dataset.scope;
  let p = location.pathname;
  if (mountPrefix && p.startsWith(mountPrefix)) p = p.slice(mountPrefix.length);
  return p.split("/").filter(Boolean)[0] || "splash";
})();

// End-to-end RTT slot. Ticks visibly via tickRtt while a user-
// initiated move (h/j/k/l/u) is in flight; cleared on ack. Liveness
// no longer rides this signal -- it's owned by /presence/ping.
let movePending = null;

const flashRed = () => {
  document.body.classList.remove("flash-red");
  void document.body.offsetWidth;  // force reflow so animation restarts
  document.body.classList.add("flash-red");
};

const rttEl = () => document.querySelector("#rtt");
const tickRtt = () => {
  if (movePending == null) return;
  rttEl()?.replaceChildren(`${Math.round(performance.now() - movePending.t)}ms`);
  requestAnimationFrame(tickRtt);
};

const move = (intent) => {
  // Each move carries a uuid the server echoes back via the
  // $lastReqId signal. window.onAck filters on that uuid so replay
  // patches from other clients aren't misattributed. Move-only --
  // liveness lives in the presence ping below.
  const reqId = crypto.randomUUID();
  movePending = { id: reqId, t: performance.now() };
  if ("hjkl".includes(intent)) {
    document.querySelector("#board-wrap")?.setAttribute("data-pending", intent);
  }
  requestAnimationFrame(tickRtt);
  return fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ playerId, gameId, intent, reqId }),
  }).then((r) => {
    if (!r.ok) {
      movePending = null;
      document.querySelector("#board-wrap")?.removeAttribute("data-pending");
      flashRed();
    }
  }).catch(() => {
    movePending = null;
    document.querySelector("#board-wrap")?.removeAttribute("data-pending");
    flashRed();
  });
};

// Connection liveness is now a pure function of /presence/ping. Each
// ping carries a {tabId, scope, gameId?} body; a 204 ack flips
// body[data-conn]=ok, anything else (non-204, fetch error, timeout)
// flips it to down. The conn indicator in layout reads this attribute
// via CSS; no SSE-driven liveness handlers anywhere.
const PING_INTERVAL_MS = 3000;
const PING_TIMEOUT_MS = 4000;
const setConn = (v) => {
  if (document.body.dataset.conn === v) return;
  document.body.dataset.conn = v;
};
const TAB_ID_KEY = "nu2048.tabId";
let tabId = sessionStorage.getItem(TAB_ID_KEY);
if (!tabId) { tabId = crypto.randomUUID(); sessionStorage.setItem(TAB_ID_KEY, tabId); }
const presencePing = async () => {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), PING_TIMEOUT_MS);
  try {
    const r = await fetch(pingUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tabId, scope, gameId }),
      signal: ctrl.signal,
    });
    setConn(r.status === 204 ? "ok" : "down");
  } catch {
    setConn("down");
  } finally {
    clearTimeout(t);
  }
};
presencePing();
setInterval(presencePing, PING_INTERVAL_MS);

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

// Below: /play-only handlers gated on moveUrl (the server-rendered
// page omits the `data-move-url` body attr on non-/play pages).
if (moveUrl) {
// Called by data-on-signal-patch="window.onAck($lastReqId)" on the
// hidden element in the /play body. The SSE pipeline ships a
// $lastReqId signal patch the instant it sees the move frame -- so
// every move (state-changing or no-op) round-trips through here.
// No-op unless reqId matches the pending probe we issued (replay /
// spectator streams carry reqIds we never issued; ignore them).
window.onAck = (reqId) => {
  // User move resolution: writes the visible RTT readout. RTT is now
  // strictly move-driven (no ping seeding) -- the readout stays empty
  // until the player presses a key.
  if (movePending && reqId === movePending.id) {
    const rtt = Math.round(performance.now() - movePending.t);
    movePending = null;
    document.querySelector("#board-wrap")?.removeAttribute("data-pending");
    rttEl()?.replaceChildren(`${rtt}ms`);
  }
};

// Keyboard: hjkl + arrows + u-to-undo. (New game lives on the splash;
// reset key is intentionally gone.)
const keymap = {
  h: "h", ArrowLeft: "h",
  j: "j", ArrowDown: "j",
  k: "k", ArrowUp: "k",
  l: "l", ArrowRight: "l",
};

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
  // Each splash-slider drag-release jumps the audio to a random spot
  // when playing -- ties the soundtrack mood to the user-driven scrub.
  // The <scrub-knob> WC emits `scrub-end` on pointer release (matches
  // the role the native `change` event used to play here).
  document.querySelector("#splash-slider")?.addEventListener("scrub-end", () => {
    if (splashAudio.paused) return;
    if (Number.isFinite(splashAudio.duration) && splashAudio.duration > 0) {
      splashAudio.currentTime = Math.random() * splashAudio.duration;
    }
  });
}
