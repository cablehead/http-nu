// Push-demo state machine.
//
// Two axes:
//   - device:  detected once at boot. Picks which OS/browser-specific copy
//              block to render. Some devices (ios-other, unsupported) have
//              no state-level content -- the device block is the whole
//              message.
//   - state:   recomputed dynamically as permission / subscription / standalone
//              changes.
//
// Why per-device rendering: a user is on one device. Showing them
// "iOS: Settings → ..., Desktop: lock icon" simultaneously is noise.

const DEVICE = Object.freeze({
  IOS_SAFARI: "ios-safari",          // install-required, then standard
  IOS_OTHER: "ios-other",            // Chrome/Firefox/Edge iOS -- WebKit but no Push API
  ANDROID: "android",                // direct; a2hs optional
  MACOS_SAFARI: "macos-safari",      // direct (Sonoma+)
  DESKTOP: "desktop",                // Chrome / Firefox / Edge desktop
  UNSUPPORTED: "unsupported",        // missing SW / PushManager / Notification
});

const STATE = Object.freeze({
  INSTALL_REQUIRED: "install-required", // iOS Safari only, pre-standalone
  AWAITING_PERMISSION: "awaiting-permission",
  SUBSCRIBING: "subscribing",
  READY: "ready",
  DENIED: "denied",
});

function detectDevice() {
  if (
    !("serviceWorker" in navigator) ||
    !("PushManager" in window) ||
    !("Notification" in window)
  ) {
    return DEVICE.UNSUPPORTED;
  }
  const ua = navigator.userAgent;
  const isIOS =
    /iPad|iPhone|iPod/.test(ua) ||
    (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
  if (isIOS) {
    // CriOS = Chrome iOS, FxiOS = Firefox iOS, EdgiOS = Edge iOS. All WebKit-
    // backed but none expose the Push API. Safari iOS is the only path.
    return /CriOS|FxiOS|EdgiOS/.test(ua) ? DEVICE.IOS_OTHER : DEVICE.IOS_SAFARI;
  }
  if (/Android/.test(ua)) return DEVICE.ANDROID;
  if (/Mac/.test(ua) && /Safari/.test(ua) && !/Chrome|Edg/.test(ua)) {
    return DEVICE.MACOS_SAFARI;
  }
  return DEVICE.DESKTOP;
}

function isStandalone() {
  return (
    navigator.standalone === true ||
    window.matchMedia("(display-mode: standalone)").matches
  );
}

async function currentSubscription() {
  if (!("serviceWorker" in navigator)) return null;
  const reg = await navigator.serviceWorker.getRegistration();
  if (!reg) return null;
  return reg.pushManager.getSubscription();
}

async function computeState(device) {
  // Terminal devices have no state-axis content.
  if (device === DEVICE.UNSUPPORTED || device === DEVICE.IOS_OTHER) return null;
  if (device === DEVICE.IOS_SAFARI && !isStandalone()) {
    return STATE.INSTALL_REQUIRED;
  }
  switch (Notification.permission) {
    case "denied":
      return STATE.DENIED;
    case "default":
      return STATE.AWAITING_PERMISSION;
    case "granted": {
      const sub = await currentSubscription();
      return sub ? STATE.READY : STATE.SUBSCRIBING;
    }
    default:
      return STATE.AWAITING_PERMISSION;
  }
}

// ---------------------------------------------------------------------------
// Subscribe / unsubscribe
// ---------------------------------------------------------------------------

function urlBase64ToUint8Array(base64String) {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const arr = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
  return arr;
}

async function registerServiceWorker() {
  return navigator.serviceWorker.register("/sw.js", { scope: "/" });
}

async function fetchVapidPublicKey() {
  const r = await fetch("/vapid-public-key");
  if (!r.ok) throw new Error(`fetch vapid: ${r.status}`);
  return (await r.text()).trim();
}

async function subscribe() {
  const reg = await navigator.serviceWorker.ready;
  const vapidPublic = await fetchVapidPublicKey();
  const sub = await reg.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: urlBase64ToUint8Array(vapidPublic),
  });
  const r = await fetch("/subscribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(sub.toJSON()),
  });
  if (!r.ok) throw new Error(`POST /subscribe: ${r.status}`);
  return sub;
}

async function unsubscribe() {
  const sub = await currentSubscription();
  if (!sub) return;
  await fetch("/unsubscribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint: sub.endpoint }),
  });
  await sub.unsubscribe();
}

async function requestPermissionAndSubscribe() {
  const perm = await Notification.requestPermission();
  if (perm !== "granted") return perm;
  await subscribe();
  return "granted";
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

function render(device, state) {
  // Hide every device + state block first.
  document
    .querySelectorAll("[data-device], [data-state]")
    .forEach((el) => (el.hidden = true));

  // Show device blocks matching the detected device (data-device can be
  // space-separated, e.g. "macos-safari desktop" for shared copy).
  document.querySelectorAll("[data-device]").forEach((el) => {
    const wants = el.dataset.device.split(/\s+/);
    if (wants.includes(device)) el.hidden = false;
  });

  // Within visible device blocks, show state blocks matching the current state.
  if (state) {
    document.querySelectorAll("[data-state]").forEach((el) => {
      const wants = el.dataset.state.split(/\s+/);
      if (wants.includes(state)) el.hidden = false;
    });
  }
}

async function updateDebug(device, state) {
  const panel = document.getElementById("debug-panel");
  if (!panel || panel.hidden) return;
  const sub = await currentSubscription();
  panel.querySelector("#dbg-device").textContent = device;
  panel.querySelector("#dbg-state").textContent = state || "(n/a)";
  panel.querySelector("#dbg-isStandalone").textContent = isStandalone();
  panel.querySelector("#dbg-permission").textContent =
    "Notification" in window ? Notification.permission : "n/a";
  panel.querySelector("#dbg-subscription").textContent = sub
    ? sub.endpoint.slice(0, 60) + "..."
    : "(none)";
}

async function refresh(device) {
  const state = await computeState(device);
  render(device, state);
  await updateDebug(device, state);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const device = detectDevice();

  if (new URLSearchParams(location.search).get("debug") === "1") {
    document.getElementById("debug-panel").hidden = false;
  }

  if (device !== DEVICE.UNSUPPORTED && device !== DEVICE.IOS_OTHER) {
    try {
      await registerServiceWorker();
    } catch (e) {
      console.error("SW register failed", e);
    }
  }

  // All Enable / Disable / Test buttons share a class so listeners attach to
  // every per-device copy. Only one is ever visible at a time, but binding
  // them all is simpler than re-binding on state change.
  document.querySelectorAll(".btn-enable").forEach((b) =>
    b.addEventListener("click", async () => {
      try {
        await requestPermissionAndSubscribe();
      } catch (e) {
        console.error(e);
        alert("Subscribe failed: " + e.message);
      }
      await refresh(device);
    }),
  );

  document.querySelectorAll(".btn-disable").forEach((b) =>
    b.addEventListener("click", async () => {
      await unsubscribe();
      await refresh(device);
    }),
  );

  document.querySelectorAll(".btn-test").forEach((b) =>
    b.addEventListener("click", async () => {
      const r = await fetch("/send-self", { method: "POST" });
      if (!r.ok) alert("Test send failed: " + r.status);
    }),
  );

  await refresh(device);

  // Re-check when tab comes back into focus (e.g. user toggled permission
  // in Settings and returned, or installed to Home Screen and re-opened).
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refresh(device);
  });
}

main();
