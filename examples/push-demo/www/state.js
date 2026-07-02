// Push-demo permission/subscribe state machine.
//
// Install UX is delegated to <pwa-install> from @khmyznikov/pwa-install. This
// file owns only the AFTER-install side: permission prompt -> subscribe ->
// ready -> test/unsubscribe. ~100 LOC.
//
// One device branch remains: iOS Chrome/Firefox/Edge (CriOS/FxiOS/EdgiOS).
// They use WebKit but don't expose the Push API even after install -- only
// Safari iOS does. <pwa-install> handles install but not browser-switching,
// so we detect and route them to a "use Safari" terminal state.
//
// Debug panel: append ?debug=1 to URL.

const STATE = Object.freeze({
  USE_SAFARI: "use-safari",          // iOS Chrome/Firefox/Edge -- never going to work
  INSTALL_REQUIRED: "install-required", // Push API not exposed; defer to <pwa-install>
  ENABLE: "enable",                  // ready to ask
  SUBSCRIBED: "subscribed",          // already registered
  DENIED: "denied",                  // permission rejected
});

function isIOSOther() {
  const ua = navigator.userAgent;
  const isIOS =
    /iPad|iPhone|iPod/.test(ua) ||
    (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
  return isIOS && /CriOS|FxiOS|EdgiOS/.test(ua);
}

function isStandalone() {
  return (
    navigator.standalone === true ||
    window.matchMedia("(display-mode: standalone)").matches
  );
}

function pushApiAvailable() {
  return (
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

async function currentSubscription() {
  if (!("serviceWorker" in navigator)) return null;
  const reg = await navigator.serviceWorker.getRegistration();
  if (!reg) return null;
  return reg.pushManager.getSubscription();
}

async function computeState() {
  if (isIOSOther()) return STATE.USE_SAFARI;
  if (!pushApiAvailable()) return STATE.INSTALL_REQUIRED;
  switch (Notification.permission) {
    case "denied":
      return STATE.DENIED;
    case "default":
      return STATE.ENABLE;
    case "granted": {
      const sub = await currentSubscription();
      return sub ? STATE.SUBSCRIBED : STATE.ENABLE;
    }
    default:
      return STATE.ENABLE;
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

function render(state) {
  document.querySelectorAll("[data-state]").forEach((el) => {
    el.hidden = el.dataset.state !== state;
  });
}

async function updateDebug(state) {
  const panel = document.getElementById("debug-panel");
  if (!panel || panel.hidden) return;
  const sub = await currentSubscription();
  panel.querySelector("#dbg-state").textContent = state;
  panel.querySelector("#dbg-pushapi").textContent = pushApiAvailable();
  panel.querySelector("#dbg-standalone").textContent = isStandalone();
  panel.querySelector("#dbg-permission").textContent =
    "Notification" in window ? Notification.permission : "n/a";
  panel.querySelector("#dbg-subscription").textContent = sub
    ? sub.endpoint.slice(0, 60) + "..."
    : "(none)";
}

async function refresh() {
  const state = await computeState();
  render(state);
  await updateDebug(state);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  if (new URLSearchParams(location.search).get("debug") === "1") {
    document.getElementById("debug-panel").hidden = false;
  }

  // Register the service worker where push will plausibly work. iOS Safari
  // supports SW pre-install (Push API is the gated bit), so we include it.
  if (!isIOSOther() && "serviceWorker" in navigator) {
    try {
      await registerServiceWorker();
    } catch (e) {
      console.error("SW register failed", e);
    }
  }

  document.querySelectorAll(".btn-enable").forEach((b) =>
    b.addEventListener("click", async () => {
      try {
        await requestPermissionAndSubscribe();
      } catch (e) {
        console.error(e);
        alert("Subscribe failed: " + e.message);
      }
      await refresh();
    }),
  );

  document.querySelectorAll(".btn-disable").forEach((b) =>
    b.addEventListener("click", async () => {
      await unsubscribe();
      await refresh();
    }),
  );

  document.querySelectorAll(".btn-test").forEach((b) =>
    b.addEventListener("click", async () => {
      const r = await fetch("/send-self", { method: "POST" });
      if (!r.ok) alert("Test send failed: " + r.status);
    }),
  );

  document.querySelectorAll(".btn-install").forEach((b) =>
    b.addEventListener("click", () => {
      const el = document.getElementById("pwa-install");
      if (el && typeof el.showDialog === "function") {
        el.showDialog(true);
      } else {
        alert("Install component not loaded yet — try refreshing.");
      }
    }),
  );

  // Recompute state when <pwa-install> fires its lifecycle events, or when
  // the page comes back into focus (e.g. user installed and re-opened from
  // home screen, or toggled permission in Settings).
  ["pwa-install-success-event", "pwa-user-choice-result-event"].forEach((ev) =>
    window.addEventListener(ev, () => refresh()),
  );
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refresh();
  });

  await refresh();
}

main();
