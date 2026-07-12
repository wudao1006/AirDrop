import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DesktopClient, Unsubscribe } from "./client";
import { DemoDesktopClient } from "./demo-client";
import type { AppSettings, PlatformKind, UiSnapshot } from "../model";
import { loadAppearanceSettings, normalizeAppearanceSettings, saveAppearanceSettings } from "../features/settings/appearance";

const SNAPSHOT_EVENT = "airdrop://snapshot";

const browserClipboardWriter = async (text: string): Promise<void> => {
  if (!navigator.clipboard) {
    throw new Error("浏览器预览不能访问系统剪贴板，请使用 Tauri 桌面程序");
  }
  await navigator.clipboard.writeText(text);
};

const browserClipboardReader = async (): Promise<string> => {
  if (!navigator.clipboard) {
    throw new Error("当前运行环境不能读取系统剪贴板");
  }
  return navigator.clipboard.readText();
};

export const detectPlatform = (): PlatformKind => {
  const override = import.meta.env.VITE_PLATFORM;
  if (override === "android") return "android";
  return /Android/i.test(navigator.userAgent) ? "android" : "desktop";
};

class TauriDesktopClient implements DesktopClient {
  readonly platform = detectPlatform();
  private appearanceInitialized = false;

  async getSnapshot(): Promise<UiSnapshot> {
    let snapshot = await invoke<UiSnapshot>("get_snapshot", {
      platform: this.platform,
      now: new Date().toISOString(),
    });
    if (!this.appearanceInitialized) {
      this.appearanceInitialized = true;
      const appearance = loadAppearanceSettings();
      await invoke("update_settings", { settings: appearance });
      snapshot = { ...snapshot, settings: { ...snapshot.settings, ...appearance } };
    }
    return snapshot;
  }

  subscribe(listener: (snapshot: UiSnapshot) => void): Unsubscribe {
    let active = true;
    let dispose: Unsubscribe | undefined;
    void listen<UiSnapshot>(SNAPSHOT_EVENT, ({ payload }) => {
      if (active) listener(payload);
    }).then((unlisten) => {
      if (!active) unlisten(); else dispose = unlisten;
    });
    return () => {
      active = false;
      dispose?.();
    };
  }

  createImportIntent(slotId: string, revision: number): Promise<string> {
    return invoke("create_import_intent", { slotId, revision });
  }

  async confirmImport(importId: string): Promise<void> {
    const text = await invoke<string>("confirm_import", { importId });
    await writeText(text);
  }

  cancelImport(importId: string): Promise<void> {
    return invoke("cancel_import", { importId });
  }

  setPause(kind: "publish" | "subscribe", paused: boolean): Promise<void> {
    return invoke("set_pause", { kind, paused });
  }

  setSynchronizationPaused(paused: boolean): Promise<void> {
    return invoke("set_synchronization_paused", { paused });
  }

  setAppActivity(activity: "foreground" | "background"): Promise<void> {
    return invoke("set_app_activity", { activity, now: new Date().toISOString() });
  }

  async publishCurrentClipboard(): Promise<void> {
    const text = await readText();
    await invoke("publish_local_clipboard", { text, now: new Date().toISOString() });
  }

  async updateSettings(settings: Partial<AppSettings>): Promise<void> {
    await invoke("update_settings", { settings });
    const appearance = normalizeAppearanceSettings({ ...loadAppearanceSettings(), ...settings });
    saveAppearanceSettings(appearance);
  }
}

export const createDesktopClient = (): DesktopClient => {
  const inTauri = Boolean(window.__TAURI_INTERNALS__);
  if (inTauri) return new TauriDesktopClient();
  return new DemoDesktopClient(
    browserClipboardWriter,
    browserClipboardReader,
    detectPlatform(),
  );
};
