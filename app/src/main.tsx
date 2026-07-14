import { StrictMode, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { FloatingOrbApp } from "./features/floating/FloatingOrbApp";
import { applyAppearanceSettings, loadAppearanceSettings } from "./features/settings/appearance";
import { createDesktopClient } from "./ipc/tauri-client";
import type { DesktopClient } from "./ipc/client";
import "./styles/tokens.css";
import "./styles/global.css";

applyAppearanceSettings(loadAppearanceSettings());

const setFloatingDocumentMode = (enabled: boolean): void => {
  if (typeof document === "undefined") return;
  document.documentElement.classList.toggle("floating-surface", enabled);
};

if (typeof location !== "undefined") {
  setFloatingDocumentMode(new URLSearchParams(location.search).get("surface") === "floating");
}

export function AirDropSurface({
  search = location.search,
  createClient = createDesktopClient,
}: {
  search?: string;
  createClient?: () => DesktopClient;
}) {
  const floating = new URLSearchParams(search).get("surface") === "floating";
  useLayoutEffect(() => {
    setFloatingDocumentMode(floating);
    return () => {
      if (floating) setFloatingDocumentMode(false);
    };
  }, [floating]);
  if (floating) return <FloatingOrbApp />;
  return <App client={createClient()} />;
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<StrictMode><AirDropSurface /></StrictMode>);
