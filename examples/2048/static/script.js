// Input handlers for 2048. Server embeds the player id and the /move URL
// as body data-* attributes so this file stays parameter-free and cacheable.
const playerId = document.body.dataset.playerId;
const gameId = document.body.dataset.gameId;
const moveUrl = document.body.dataset.moveUrl;

// End-to-end RTT: time from a move() call to the next DOM mutation in
// #game (i.e. when the SSE patch lands). The mean is published on
// :root as --rtt-mean so CSS dials can scale animation timing with
// latency.
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
  // Light the directional edge glow for the duration of the round trip.
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
    rtts.length = 0;
    rttEl()?.replaceChildren("");
  }
  if (prevConn === "down" && v === "ok") {
    document.body.classList.remove("reconnect-pulse");
    void document.body.offsetWidth;
    document.body.classList.add("reconnect-pulse");
    // Re-probe RTT after reconnect. (Only on /play -- /games has no move URL.)
    if (moveUrl) move("");
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
new MutationObserver(() => {
  if (!initSeen) {
    // First mutation is the SSE init render. Now that the stream's open,
    // send a no-op ping to seed the RTT estimate.
    initSeen = true;
    move("");
    return;
  }
  if (pending == null) return;
  // Only attribute the mutation to our pending probe if #game's data-rev
  // matches the reqId we issued. Replay patches use a fresh-uuid data-rev
  // so they don't get misattributed.
  if (game.dataset.rev !== pending.id) return;
  const rtt = Math.round(performance.now() - pending.t);
  pending = null;
  // SSE patch landed: clear pointer-drag glow and the pending indicator.
  const w = document.querySelector("#board-wrap");
  w?.style.setProperty("--glow-x", "0px");
  w?.style.setProperty("--glow-y", "0px");
  w?.removeAttribute("data-pending");
  document.querySelector("#rtt")?.replaceChildren(`${rtt}ms`);
  rtts.push(rtt);
  if (rtts.length > RTT_HISTORY) rtts.shift();
  // The spring curve peaks around 40% through the animation. To make that
  // peak land near (or just past) SSE arrival, target ~2x mean RTT so the
  // overshoot bounce is still playing when view-transition picks up.
  const mean = rtts.reduce((a, b) => a + b, 0) / rtts.length;
  // Expose the mean RTT to CSS as a unitless number. The animation
  // duration / bezier vars in styles.css clamp `base + k*--rtt-mean` to
  // do their own latency scaling, so we don't set --decay-duration etc
  // directly any more -- it all comes out of the CSS dials.
  document.documentElement.style.setProperty("--rtt-mean", String(mean));
}).observe(game, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-rev"] });

// Keyboard: hjkl + arrows + u-to-undo. (New game lives on the splash;
// reset key is intentionally gone.)
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
// Global navigation: Esc -> splash, n -> new game. Registered always so
// every page (play, watch, my games, splash, notes, design) responds.
addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    location.href = "/";
    e.preventDefault();
    return;
  }
  if (e.key === "n" && !e.ctrlKey && !e.metaKey && !e.altKey) {
    location.href = "/new";
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
      if (glowFor[intent]) {
        const [prop, val] = glowFor[intent];
        const w = document.querySelector("#board-wrap");
        w?.style.setProperty(prop, `${val}px`);
        void w;
      }
      move(intent);
      e.preventDefault();
    }
  });
}

// Delegated click handlers for the kbd-btn family. All kbd-btns are
// <button> (uniform styling, no <a>/<button> drift) and carry their
// behavior on a data attribute:
//   [data-intent]  game move (move keys, undo)
//   [data-href]    navigation shortcut (esc -> /, n -> /new)
document.addEventListener("click", (e) => {
  const intent = e.target.closest("button[data-intent]");
  if (intent) { move(intent.dataset.intent); return; }
  const nav = e.target.closest("button[data-href]");
  if (nav) { location.href = nav.dataset.href; }
});



// Swipe with anticipation: while dragging, lean the tiles toward the gesture
// (CSS reads --tilt-x / --tilt-y). On release past threshold, leave the lean
// in place so the view-transition slide continues the motion. On cancel,
// snap back via the `.snap` class.
const DAMP = 0.9;
const CAP = 26;
const AXIS_LOCK = 8;  // once you move this far on one axis, the other locks
let start = null;
let wrap = null;
let axis = null;
let committedDir = null;     // direction the swipe committed to (null until threshold crossed)

addEventListener("pointerdown", (e) => {
  if (!e.target.closest(".board")) { start = null; return; }
  start = [e.clientX, e.clientY];
  axis = null;
  committedDir = null;
  wrap = document.querySelector("#board-wrap");
  wrap?.classList.remove("snap", "decay");
});

addEventListener("pointermove", (e) => {
  if (!start || !wrap) return;
  if (committedDir) return;  // committed -- ignore further movement until release
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
  // Commit on first threshold crossing -- the impulse fires on release;
  // tilt + glow release with a spring so the board visibly settles back to
  // its origin and the incoming SSE patches animate over a still board.
  if (Math.max(Math.abs(dx), Math.abs(dy)) >= 30) {
    committedDir = Math.abs(dx) > Math.abs(dy)
      ? (dx > 0 ? "l" : "h")
      : (dy > 0 ? "j" : "k");
    wrap.style.setProperty("--tilt-x", "0px");
    wrap.style.setProperty("--tilt-y", "0px");
    const LIT = 5;  // alpha ~0.6 via /8 in the gradient
    wrap.style.setProperty("--glow-x",
      committedDir === "l" ? `${LIT}px` :
      committedDir === "h" ? `${-LIT}px` : "0px");
    wrap.style.setProperty("--glow-y",
      committedDir === "j" ? `${LIT}px` :
      committedDir === "k" ? `${-LIT}px` : "0px");
    wrap.classList.add("decay");
  }
});

addEventListener("pointerup", () => {
  if (!start) return;
  start = null;
  if (!committedDir) {
    // Below threshold: cancel, spring tilt + glow back to rest.
    wrap?.style.setProperty("--tilt-x", "0px");
    wrap?.style.setProperty("--tilt-y", "0px");
    wrap?.style.setProperty("--glow-x", "0px");
    wrap?.style.setProperty("--glow-y", "0px");
    wrap?.classList.add("snap");
    setTimeout(() => wrap?.classList.remove("snap"), 260);
  } else {
    // Committed and released -- send the impulse now. Lit edge stays until
    // SSE clears it.
    move(committedDir);
  }
  committedDir = null;
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
  const toggleAudio = () => {
    if (splashAudio.paused) splashAudio.play();
    else splashAudio.pause();
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
