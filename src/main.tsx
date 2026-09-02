import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { backend } from "./services/backend";
import "./styles.css";

function reportUncaught(message: string, stack: string | null) {
  const context = `userAgent=${navigator.userAgent} viewport=${window.innerWidth}x${window.innerHeight} dpr=${window.devicePixelRatio}`;
  try { void backend.reportInterfaceError(message, stack, context).catch(() => {}); }
  catch { /* The browser development build has no native bridge. */ }
}

window.addEventListener("error", event => {
  reportUncaught(event.message || "Unknown window error", event.error instanceof Error ? event.error.stack ?? null : null);
});
window.addEventListener("unhandledrejection", event => {
  const error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));
  reportUncaught(error.message, error.stack ?? null);
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><ErrorBoundary><App /></ErrorBoundary></React.StrictMode>
);
