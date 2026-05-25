// Push-demo state machine.
//
// Determines which UI state to render based on:
//   - is the browser iOS Safari?
//   - is the site running standalone (installed from Home Screen)?
//   - what's Notification.permission?
//   - does the user already have a PushSubscription?
//
// On iOS 16.4+, web push REQUIRES install-to-Home-Screen. There is no
// `beforeinstallprompt` event on iOS Safari -- you cannot programmatically
// trigger or detect installation. You can only detect, after the fact, that
// the user opened the app from the home screen (navigator.standalone === true).
//
// Debug panel: append ?debug=1 to URL.

const STATES = {
  UNSUPPORTED: "unsupported",
  IOS_INSTALL_REQUIRED: "ios-install-required",
  AWAITING_PERMISSION: "awaiting-permission",
  SUBSCRIBING: "subscribing",
  READY: "ready",
  DENIED: "denied",
  ERROR: "error",
};

const isIOS = () =>
  /iPad|iPhone|iPod/.test(navigator.userAgent) ||
  (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);

const isStandalone = () =>
  navigator.standalone === true ||
  window.matchMedia("(display-mode: standalone)").matches;

const isSupported = () =>
  "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;

// Convert a URL-safe base64 string (VAPID public key from server) to the
// Uint8Array that pushManager.subscribe wants for applicationServerKey.
function urlBase64ToUint8Array(base64String) {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const arr = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
  return arr;
}

async function currentSubscription() {
  if (!("serviceWorker" in navigator)) return null;
  const reg = await navigator.serviceWorker.getRegistration();
  if (!reg) return null;
  return reg.pushManager.getSubscription();
}

async function computeState() {
  if (!isSupported()) return STATES.UNSUPPORTED;
  if (isIOS() && !isStandalone()) return STATES.IOS_INSTALL_REQUIRED;

  switch (Notification.permission) {
    case "denied":
      return STATES.DENIED;
    case "default":
      return STATES.AWAITING_PERMISSION;
    case "granted": {
      const sub = await currentSubscription();
      return sub ? STATES.READY : STATES.SUBSCRIBING;
    }
    default:
      return STATES.ERROR;
  }
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
// UI rendering
// ---------------------------------------------------------------------------

function $(id) {
  return document.getElementById(id);
}

function showState(state) {
  document.querySelectorAll("[data-state]").forEach((el) => {
    el.hidden = el.dataset.state !== state;
  });
  $("debug-state") && ($("debug-state").textContent = state);
}

async function refresh() {
  const state = await computeState();
  showState(state);
  await updateDebug();
}

async function updateDebug() {
  const panel = $("debug-panel");
  if (!panel) return;
  const sub = await currentSubscription();
  panel.querySelector("#dbg-isIOS").textContent = isIOS();
  panel.querySelector("#dbg-isStandalone").textContent = isStandalone();
  panel.querySelector("#dbg-supported").textContent = isSupported();
  panel.querySelector("#dbg-permission").textContent =
    "Notification" in window ? Notification.permission : "n/a";
  panel.querySelector("#dbg-subscription").textContent = sub
    ? sub.endpoint.slice(0, 60) + "..."
    : "(none)";
}

async function main() {
  if (new URLSearchParams(location.search).get("debug") === "1") {
    $("debug-panel").hidden = false;
  }

  if (isSupported()) {
    try {
      await registerServiceWorker();
    } catch (e) {
      console.error("SW register failed", e);
    }
  }

  $("btn-enable")?.addEventListener("click", async () => {
    try {
      await requestPermissionAndSubscribe();
    } catch (e) {
      console.error(e);
      alert("Subscribe failed: " + e.message);
    }
    await refresh();
  });

  $("btn-disable")?.addEventListener("click", async () => {
    await unsubscribe();
    await refresh();
  });

  $("btn-test")?.addEventListener("click", async () => {
    const r = await fetch("/send-self", { method: "POST" });
    if (!r.ok) alert("Test send failed: " + r.status);
  });

  await refresh();

  // Re-check state when the page comes back into focus (e.g. user
  // toggled permission in Settings and returned).
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refresh();
  });
}

main();
