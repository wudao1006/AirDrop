import { useCallback, useEffect, useLayoutEffect, useMemo, useState } from "react";
import type { DesktopClient } from "../ipc/client";
import { EMPTY_TELEMETRY, type PageId, type TelemetrySnapshot, type UiSnapshot } from "../model";
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
import { listen } from "@tauri-apps/api/event";
import { Icon } from "../components/Icon";
import { ErrorToast } from "../components/ErrorToast";

export function App({ client }: { client: DesktopClient }) {
  const [page, setPage] = useState<PageId>("home");
  const [snapshot, setSnapshot] = useState<UiSnapshot | null>(null);
  const [telemetry, setTelemetry] = useState<TelemetrySnapshot>(EMPTY_TELEMETRY);
  const [error, setError] = useState<string | null>(null);
  const clearError = useCallback(() => setError(null), []);
  useAndroidLifecycle(client);
  const observesTelemetry = page === "devices" || page === "transfers";

  useEffect(() => {
    let mounted = true;
    const applySnapshot = (value: UiSnapshot) => {
      if (!mounted) return;
      setSnapshot((current) => !current || value.revision > current.revision ? value : current);
    };
    const unsubscribe = client.subscribe(applySnapshot);
    void client.getSnapshot().then(applySnapshot).catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "AirDrop 启动失败"));
    return () => { mounted = false; unsubscribe(); };
  }, [client]);

  useEffect(() => {
    if (!observesTelemetry) return;
    let mounted = true;
    const applyTelemetry = (value: TelemetrySnapshot) => {
      if (!mounted) return;
      setTelemetry((current) => {
        const nextSampledAt = Date.parse(value.sampledAt);
        const currentSampledAt = Date.parse(current.sampledAt);
        if (Number.isNaN(nextSampledAt)) return current;
        return Number.isNaN(currentSampledAt) || nextSampledAt >= currentSampledAt ? value : current;
      });
    };
    const unsubscribe = client.subscribeTelemetry(applyTelemetry);
    void client.setTelemetryObserving(true).catch(() => undefined);
    void client.getTelemetry().then(applyTelemetry).catch(() => undefined);
    return () => {
      mounted = false;
      unsubscribe();
      void client.setTelemetryObserving(false).catch(() => undefined);
    };
  }, [client, observesTelemetry]);

  useEffect(() => {
    if (client.platform !== "desktop" || !window.__TAURI_INTERNALS__) return;
    let active = true;
    let dispose: (() => void) | undefined;
    void listen("airdrop://open-clipboard", () => setPage("clipboard")).then((unlisten) => {
      if (active) dispose = unlisten; else unlisten();
    });
    return () => {
      active = false;
      dispose?.();
    };
  }, [client.platform]);

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
      case "devices": return <DevicesPage snapshot={snapshot} telemetry={telemetry} client={client} onError={setError} />;
      case "groups": return <GroupsPage snapshot={snapshot} client={client} onError={setError} />;
      case "transfers": return <TransfersPage snapshot={snapshot} telemetry={telemetry} />;
      case "settings": return <SettingsPage {...shared} />;
    }
  }, [client, page, snapshot, telemetry]);

  if (!snapshot) return <div className="splash"><div className="brand-mark"><span>✦</span></div><strong>正在启动 AirDrop</strong><span>{error ?? "正在准备…"}</span></div>;

  return <AppShell page={page} setPage={setPage} snapshot={snapshot}>
    {client.platform === "desktop" && <FloatingOrbManager client={client} snapshot={snapshot} setPage={setPage} onError={setError} />}
    {error && <ErrorToast message={error} onClose={clearError} />}
    {page !== "devices" && snapshot.pendingPairings.length > 0 && <div className="request-banner" role="status"><Icon name="shield" size={18} /><div><strong>收到设备配对请求</strong><span>{snapshot.pendingPairings[0].deviceName} 正在等待本机核对验证码</span></div><button type="button" className="button" onClick={() => setPage("devices")}>查看请求</button></div>}
    {page !== "groups" && snapshot.pendingGroupInvites.length > 0 && <div className="request-banner" role="status"><Icon name="groups" size={18} /><div><strong>收到同步组邀请</strong><span>{snapshot.pendingGroupInvites[0].ownerName} 邀请本机加入“{snapshot.pendingGroupInvites[0].groupName}”</span></div><button type="button" className="button primary" onClick={() => setPage("groups")}>查看并处理</button></div>}
    {content}
  </AppShell>;
}
