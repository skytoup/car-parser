import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import "./app.css";
import "./i18n";

async function cleanupLegacyPwaRegistrations(): Promise<void> {
  if (
    typeof window === "undefined" ||
    !("serviceWorker" in navigator)
  ) {
    return;
  }

  try {
    const registrations = await navigator.serviceWorker.getRegistrations();
    const staleRegistrations = registrations.filter((registration) => {
      const scriptUrl =
        registration.active?.scriptURL ??
        registration.waiting?.scriptURL ??
        registration.installing?.scriptURL ??
        "";

      return /vite-plugin-pwa|dev-sw|workbox|\/sw\.js(?:\?|$)/.test(scriptUrl);
    });

    await Promise.all(
      staleRegistrations.map((registration) => registration.unregister()),
    );

    if (!("caches" in window)) {
      return;
    }

    const cacheNames = await window.caches.keys();
    const staleCacheNames = cacheNames.filter((name) =>
      /vite-plugin-pwa|workbox|^pwa-/i.test(name),
    );

    await Promise.all(
      staleCacheNames.map((cacheName) => window.caches.delete(cacheName)),
    );
  } catch {
    // Ignore cleanup failures: the app does not depend on service workers.
  }
}

void cleanupLegacyPwaRegistrations();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
