import React from "react";
import ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/react";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./index.css";
import packageJson from "../package.json";

try {
  const savedLang = localStorage.getItem("deepnode_copy_lang") || "tr";
  document.documentElement.lang = savedLang;
  document.documentElement.dir = savedLang === "ar" ? "rtl" : "ltr";
} catch {
  document.documentElement.lang = "tr";
}

const SENTRY_DSN =
  import.meta.env.VITE_SENTRY_DSN ||
  "https://b475ca7076c2499baac1d734301acaff@o4512196697128960.ingest.de.sentry.io/4512196703420496";

Sentry.init({
  dsn: SENTRY_DSN,
  release: `deepnode-copy@${packageJson.version}`,
  environment: import.meta.env.DEV ? "development" : "production",
  tracesSampleRate: 1.0,
});

window.addEventListener("error", (e) => {
  invoke("log_js", { msg: `WINDOW ERROR: ${e.message} at ${e.filename}:${e.lineno}` }).catch(() => {});
  Sentry.captureException(e.error || e.message);
});

window.addEventListener("unhandledrejection", (e) => {
  invoke("log_js", { msg: `UNHANDLED REJECTION: ${e.reason}` }).catch(() => {});
  Sentry.captureException(e.reason);
});

invoke("log_js", { msg: "main.tsx loaded with Sentry initialized, rendering React root..." }).catch((err) => {
  console.error("invoke log_js failed:", err);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Sentry.ErrorBoundary
      fallback={({ error }) => (
        <div className="w-screen h-screen flex flex-col items-center justify-center bg-neutral-900 text-neutral-100 p-6 text-center select-none font-sans">
          <h1 className="text-lg font-bold text-rose-500 mb-2">Beklenmeyen bir hata oluştu</h1>
          <p className="text-xs text-neutral-400 font-mono mb-4 max-w-md break-words bg-neutral-950 p-3 rounded border border-neutral-800">
            {error?.toString() || "Bilinmeyen hata"}
          </p>
          <button
            onClick={() => window.location.reload()}
            className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold rounded-lg shadow transition"
          >
            Uygulamayı Yeniden Yükle
          </button>
        </div>
      )}
    >
      <App />
    </Sentry.ErrorBoundary>
  </React.StrictMode>
);
