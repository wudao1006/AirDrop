import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import type { DesktopClient } from "../ipc/client";
import type { PageId, UiSnapshot } from "../model";
import { AppShell } from "./AppShell";
import { HomePage } from "../features/home/HomePage";
import { ClipboardPage } from "../features/clipboard/ClipboardPage";
import { DevicesPage } from "../features/devices/DevicesPage";
import { GroupsPage } from "../features/groups/GroupsPage";
import { TransfersPage } from "../features/transfers/TransfersPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { applyAppearanceSettings, subscribeToSystemAppearanceChanges } from "../features/settings/appearance";
import { useAndroidLifecycle } from "../platform/android/useAndroidLifecycle";
import { FloatingOrbManager } from "../features/floating/FloatingOrbManager";

export function App({ client }: { client: DesktopClient }) {
  const [page, setPage] = useState<PageId>("home");
  const [snapshot, setSnapshot] = useState<UiSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  useAndroidLifecycle(client);

  useEffect(() => {
    let mounted = true;
    const applySnapshot = (value: UiSnapshot) => {
      if (!mounted) return;
      setSnapshot((current) => !current || value.revision > current.revision ? value : current);
    };
    void client.getSnapshot().then(applySnapshot).catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "AirDrop 启动失败"));
    const unsubscribe = client.subscribe(applySnapshot);
    return () => { mounted = false; unsubscribe(); };
  }, [client]);

  useLayoutEffect(() => {
    if (!snapshot) return;
    applyAppearanceSettings(snapshot.settings);
    return subscribeToSystemAppearanceChanges(snapshot.settings);
  }, [snapshot]);

  const content = useMemo(() => {
    if (!snapshot) return null;
    const shared = { snapshot, client, onError: setError };
    switch (page) {
      case "home": return <HomePage {...shared} openClipboard={() => setPage("clipboard")} />;
      case "clipboard": return <ClipboardPage {...shared} />;
      case "devices": return <DevicesPage snapshot={snapshot} client={client} onError={setError} />;
      case "groups": return <GroupsPage snapshot={snapshot} client={client} onError={setError} />;
      case "transfers": return <TransfersPage />;
      case "settings": return <SettingsPage {...shared} />;
    }
  }, [client, page, snapshot]);

  if (!snapshot) return <div className="splash"><div className="brand-mark"><span>✦</span></div><strong>正在启动 AirDrop</strong><span>{error ?? "正在准备…"}</span></div>;

  return <AppShell page={page} setPage={setPage} snapshot={snapshot}>
    {client.platform === "desktop" && <FloatingOrbManager client={client} snapshot={snapshot} setPage={setPage} onError={setError} />}
    {error && <div className="toast-error" role="alert">{error}<button type="button" onClick={() => setError(null)}>关闭</button></div>}
    {content}
  </AppShell>;
}
