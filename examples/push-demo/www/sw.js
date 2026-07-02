// Service worker for http-nu push-demo.
//
// Three responsibilities:
//   1. Activate on install without waiting for tabs to close (skipWaiting +
//      clients.claim) -- otherwise new SW versions sit pending for hours.
//   2. Handle 'push' events by calling registration.showNotification.
//      Keep options minimal: actions/image/requireInteraction are IGNORED
//      on iOS, so don't bother. Title + body + icon + badge is the safe set.
//   3. Handle 'notificationclick' by focusing an existing window or opening
//      a new one. WITHOUT this handler, tapping a notification on iOS does
//      nothing -- the single most common iOS push bug.
//
// No fetch handler -- this demo doesn't cache anything. Cloudflare can do
// its own caching at the edge if needed (but we explicitly bust sw.js
// caching via Cache-Control: no-cache on the origin response).

self.addEventListener("install", (event) => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("push", (event) => {
  let data = { title: "Push", body: "(empty)" };
  if (event.data) {
    try {
      data = event.data.json();
    } catch {
      data = { title: "Push", body: event.data.text() };
    }
  }
  const title = data.title || "Push";
  const options = {
    body: data.body || "",
    icon: "/icons/192.png",
    badge: "/icons/192.png",
    tag: data.tag, // groups together by tag if provided
    data: data, // forwarded to notificationclick
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const target = (event.notification.data && event.notification.data.url) || "/";

  event.waitUntil(
    (async () => {
      const clientsList = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      // Focus an existing window if one is already at the target URL.
      for (const client of clientsList) {
        if (client.url.endsWith(target) && "focus" in client) {
          return client.focus();
        }
      }
      // Otherwise open a new one.
      if (self.clients.openWindow) {
        return self.clients.openWindow(target);
      }
    })(),
  );
});
