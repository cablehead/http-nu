// Input handlers for 2048. Server embeds the player id and the /move URL
// as body data-* attributes so this file stays parameter-free and cacheable.
const playerId = document.body.dataset.playerId;
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

const rttEl = () => document.querySelector("#rtt");
const tickRtt = () => {
  if (pending == null) return;
  rttEl()?.replaceChildren(`rtt ${Math.round(performance.now() - pending.t)}ms`);
  requestAnimationFrame(tickRtt);
};

const move = (intent) => {
  // Each move carries a uuid the server echoes back via #game's data-rev.
  // The observer only counts an RTT when data-rev matches our pending id,
  // so reconnect-replay patches don't get misattributed to a probe.
  const reqId = crypto.randomUUID();
  pending = { id: reqId, t: performance.now() };
  requestAnimationFrame(tickRtt);  // live-tick the RTT indicator while in flight
  return fetch(moveUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ playerId, intent, reqId }),
  }).then((r) => {
    if (!r.ok) { pending = null; flashRed(); }
  }).catch(() => {
    pending = null;
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
    // Old RTT readings are stale across a disconnect; clear the indicator
    // and the history so reconnect probes for a fresh measurement.
    pending = null;
    rtts.length = 0;
    rttEl()?.replaceChildren("");
    // #replay is datastar-bound (data-text) so we can't clear it from JS
    // -- the next heartbeat would re-apply $replayMs. CSS hides it while
    // body[data-conn="down"] (see styles.css).
  }
  if (prevConn === "down" && v === "ok") {
    document.body.classList.remove("reconnect-pulse");
    void document.body.offsetWidth;
    document.body.classList.add("reconnect-pulse");
    // Re-probe RTT after reconnect.
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
  // SSE patch landed: release the "still lit" edge glow. The patch's own
  // .edge-flash element carries the per-step visual from here on.
  const w = document.querySelector("#board-wrap");
  w?.style.setProperty("--glow-x", "0px");
  w?.style.setProperty("--glow-y", "0px");
  document.querySelector("#rtt")?.replaceChildren(`rtt ${rtt}ms`);
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
  // Shift+letter sends uppercase ("H"), so fall back to the lowercased key.
  // Arrow keys aren't affected (e.key is "ArrowLeft" regardless of Shift).
  const dir = keymap[e.key] || keymap[(e.key + "").toLowerCase()];
  const intent = dir
    || (e.key === "r" ? "reset" : "")
    || (e.key === "u" ? "undo" : "");
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
    // Shift + arrow triggers a slam: slide until the board stops changing.
    move(dir && e.shiftKey ? `slam-${dir}` : intent);
    e.preventDefault();
  }
});

// Reset button lives in the hint paragraph (static); the settings toggle
// lives inside #game and gets morphed in/out -- delegated handler catches
// both via event bubbling.
document.querySelectorAll("p.hint button[data-intent]").forEach((b) => {
  b.addEventListener("click", () => move(b.dataset.intent));
});

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
    body: JSON.stringify({ playerId, mode: t.dataset.viewTo }),
  });
  setTimeout(() => document.documentElement.classList.remove("view-flipping"), 600);
});


// Swipe with anticipation: while dragging, lean the tiles toward the gesture
// (CSS reads --tilt-x / --tilt-y). On release past threshold, leave the lean
// in place so the view-transition slide continues the motion. On cancel,
// snap back via the `.snap` class.
const DAMP = 0.9;
const CAP = 26;
const AXIS_LOCK = 8;  // once you move this far on one axis, the other locks
const HOLD_FOR_SLAM_MS = 500;  // hold after the swipe for this long -> slam
let start = null;
let wrap = null;
let axis = null;
let committedDir = null;     // direction the swipe committed to (null until threshold crossed)
let holdSlamTimer = null;   // fires slam-<dir> if the user keeps holding
let blastFired = false;      // hold-timer already fired a slam -- skip single on release
let touchDot = null;         // visual dot under the finger during charge-up
let settleTimer = null;      // fires startChargeUp once the finger stops moving
let lastClientX = 0, lastClientY = 0;  // tracked so charge-up spawns under finger
const SETTLE_MS = 120;       // idle this long after commit -> swipe is done

const removeTouchDot = () => {
  if (touchDot) { touchDot.remove(); touchDot = null; }
};

const armSettleTimer = () => {
  if (settleTimer) clearTimeout(settleTimer);
  settleTimer = setTimeout(() => { settleTimer = null; startChargeUp(); }, SETTLE_MS);
};

// Called once the finger has been still for SETTLE_MS after a swipe commits.
// Spawns the dot at the current finger position, lights the board edge, and
// arms the hold timer.
const startChargeUp = () => {
  if (!start || !committedDir || holdSlamTimer || touchDot || !wrap) return;
  touchDot = document.createElement("div");
  touchDot.className = "touch-dot-pos";
  touchDot.style.transform = `translate3d(${lastClientX}px, ${lastClientY}px, 0)`;
  const inner = document.createElement("div");
  inner.className = "touch-dot";
  touchDot.appendChild(inner);
  document.body.appendChild(touchDot);
  wrap.dataset.charge = committedDir;
  wrap.classList.add("charging");
  holdSlamTimer = setTimeout(() => {
    holdSlamTimer = null;
    blastFired = true;
    wrap?.classList.remove("charging");
    delete wrap?.dataset.charge;
    removeTouchDot();
    // Relight the committed edge so the user sees "slam request in flight"
    // -- the observer clears it when the first SSE patch (or no-op echo)
    // lands.
    const LIT = 5;
    wrap?.style.setProperty("--glow-x",
      committedDir === "l" ? `${LIT}px` :
      committedDir === "h" ? `${-LIT}px` : "0px");
    wrap?.style.setProperty("--glow-y",
      committedDir === "j" ? `${LIT}px` :
      committedDir === "k" ? `${-LIT}px` : "0px");
    move(`slam-${committedDir}`);
  }, HOLD_FOR_SLAM_MS);
};

addEventListener("pointerdown", (e) => {
  if (!e.target.closest("#board")) { start = null; return; }
  start = [e.clientX, e.clientY];
  lastClientX = e.clientX;
  lastClientY = e.clientY;
  axis = null;
  committedDir = null;
  blastFired = false;
  if (holdSlamTimer) { clearTimeout(holdSlamTimer); holdSlamTimer = null; }
  if (settleTimer) { clearTimeout(settleTimer); settleTimer = null; }
  removeTouchDot();
  wrap = document.querySelector("#board-wrap");
  wrap?.classList.remove("snap", "decay", "charging");
  delete wrap?.dataset.charge;
});

addEventListener("pointermove", (e) => {
  if (!start || !wrap) return;
  lastClientX = e.clientX;
  lastClientY = e.clientY;
  let dx = e.clientX - start[0];
  let dy = e.clientY - start[1];
  if (!axis && Math.max(Math.abs(dx), Math.abs(dy)) >= AXIS_LOCK) {
    axis = Math.abs(dx) > Math.abs(dy) ? "h" : "v";
  }
  if (axis === "h") dy = 0;
  if (axis === "v") dx = 0;
  // Once committed, freeze the tilt and let the board settle -- subsequent
  // pointermoves don't keep "holding" the board at the lean. The touch-dot
  // still tracks the finger so the charge-up visual stays under it.
  if (committedDir) {
    if (touchDot) {
      touchDot.style.transform = `translate3d(${e.clientX}px, ${e.clientY}px, 0)`;
    } else if (settleTimer) {
      // Still moving -- defer charge-up until the finger settles.
      armSettleTimer();
    }
    return;
  }
  const tx = Math.max(-CAP, Math.min(CAP, dx * DAMP));
  const ty = Math.max(-CAP, Math.min(CAP, dy * DAMP));
  wrap.style.setProperty("--tilt-x", `${tx}px`);
  wrap.style.setProperty("--tilt-y", `${ty}px`);
  wrap.style.setProperty("--glow-x", `${tx}px`);
  wrap.style.setProperty("--glow-y", `${ty}px`);
  // Commit on first threshold crossing -- single impulse fires immediately,
  // tilt + glow release with a spring so the board visibly settles back to
  // its origin and the incoming SSE patches animate over a still board.
  // Then arm a hold timer: if the user keeps the finger down for
  // HOLD_FOR_SLAM_MS, fire slam-<dir> to keep sliding until settled.
  if (Math.max(Math.abs(dx), Math.abs(dy)) >= 30) {
    committedDir = Math.abs(dx) > Math.abs(dy)
      ? (dx > 0 ? "l" : "h")
      : (dy > 0 ? "j" : "k");
    // Don't fire the impulse yet -- wait for release (single) or for the
    // hold-timer to fire (blast). The lit edge below shows "armed", staying
    // up until SSE clears it.
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
    // Don't start the charge-up while the finger is still completing the
    // swipe gesture -- arm a settle timer that fires once movement stops.
    armSettleTimer();
  }
});

addEventListener("pointerup", () => {
  if (!start) return;
  start = null;
  if (holdSlamTimer) { clearTimeout(holdSlamTimer); holdSlamTimer = null; }
  if (settleTimer) { clearTimeout(settleTimer); settleTimer = null; }
  // Cancel charge-up if user released before it fired.
  wrap?.classList.remove("charging");
  delete wrap?.dataset.charge;
  removeTouchDot();
  if (!committedDir) {
    // Below threshold: cancel, spring tilt + glow back to rest.
    wrap?.style.setProperty("--tilt-x", "0px");
    wrap?.style.setProperty("--tilt-y", "0px");
    wrap?.style.setProperty("--glow-x", "0px");
    wrap?.style.setProperty("--glow-y", "0px");
    wrap?.classList.add("snap");
    setTimeout(() => wrap?.classList.remove("snap"), 260);
  } else if (!blastFired) {
    // Committed and released before the hold-timer fired -- send the
    // single impulse now. Lit edge stays until SSE clears it.
    move(committedDir);
  }
  committedDir = null;
  blastFired = false;
});
