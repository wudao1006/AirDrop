import { useEffect } from "react";
import type { DesktopClient } from "../ipc/client";

export function useAppLifecycle(client: DesktopClient): void {
  useEffect(() => {
    if (client.platform !== "android") return;

    const syncVisibility = () => {
      void client.setAppActivity(document.visibilityState === "hidden" ? "background" : "foreground");
    };
    document.addEventListener("visibilitychange", syncVisibility);
    syncVisibility();
    return () => document.removeEventListener("visibilitychange", syncVisibility);
  }, [client]);
}
