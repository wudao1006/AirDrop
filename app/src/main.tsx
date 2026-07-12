import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { FloatingOrbApp } from "./features/floating/FloatingOrbApp";
import { applyAppearanceSettings, loadAppearanceSettings } from "./features/settings/appearance";
import { createDesktopClient } from "./ipc/tauri-client";
import type { DesktopClient } from "./ipc/client";
import "./styles/tokens.css";
import "./styles/global.css";

applyAppearanceSettings(loadAppearanceSettings());

export function AirDropSurface({
  search = location.search,
  createClient = createDesktopClient,
}: {
  search?: string;
  createClient?: () => DesktopClient;
}) {
  if (new URLSearchParams(search).get("surface") === "floating") return <FloatingOrbApp />;
  return <App client={createClient()} />;
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<StrictMode><AirDropSurface /></StrictMode>);
