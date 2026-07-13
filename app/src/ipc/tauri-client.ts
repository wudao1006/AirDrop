import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DesktopClient, Unsubscribe } from "./client";
import { DemoDesktopClient } from "./demo-client";
import type { AppSettings, UiSnapshot } from "../model";
import { loadAppearanceSettings, normalizeAppearanceSettings, saveAppearanceSettings } from "../features/settings/appearance";
import { detectPlatform } from "../platform/runtime";

const SNAPSHOT_EVENT = "airdrop://snapshot";

const browserClipboardWriter = async (text: string): Promise<void> => {
  if (!navigator.clipboard) {
    throw new Error("浏览器预览不能访问系统剪贴板，请打开 AirDrop 应用");
  }
  await navigator.clipboard.writeText(text);
};

const browserClipboardReader = async (): Promise<string> => {
  if (!navigator.clipboard) {
    throw new Error("当前运行环境不能读取系统剪贴板");
  }
  return navigator.clipboard.readText();
};

class TauriAppClient implements DesktopClient {
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
    await invoke("confirm_import", { importId });
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

  allowPairing(): Promise<void> {
    return invoke("allow_pairing");
  }

  beginPairing(instanceId: string): Promise<void> {
    return invoke("begin_pairing", { instanceId });
  }

  confirmPairing(pairingId: string, accepted: boolean): Promise<void> {
    return invoke("confirm_pairing", { pairingId, accepted });
  }

  setDeviceSyncEnabled(deviceId: string, enabled: boolean): Promise<void> {
    return invoke("set_device_sync_enabled", { deviceId, enabled });
  }

  revokeDevice(deviceId: string): Promise<void> {
    return invoke("revoke_device", { deviceId });
  }

  createSyncGroup(input: { name: string; memberDeviceIds: string[]; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<string> {
    return invoke("create_sync_group", { input });
  }

  confirmGroupInvite(inviteId: string, accepted: boolean): Promise<void> {
    return invoke("confirm_group_invite", { inviteId, accepted });
  }

  setGroupMemberDirection(groupId: string, deviceId: string, direction: "disabled" | "send_only" | "receive_only" | "bidirectional"): Promise<void> {
    return invoke("set_group_member_direction", { groupId, deviceId, direction });
  }

  removeGroupMember(groupId: string, deviceId: string): Promise<void> {
    return invoke("remove_group_member", { groupId, deviceId });
  }

  updateGroupPolicy(input: { groupId: string; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<void> {
    return invoke("update_group_policy", { input });
  }

  leaveSyncGroup(groupId: string): Promise<void> {
    return invoke("leave_sync_group", { groupId });
  }

  deleteSyncGroup(groupId: string): Promise<void> {
    return invoke("delete_sync_group", { groupId });
  }
}

export const createDesktopClient = (): DesktopClient => {
  const inTauri = Boolean(window.__TAURI_INTERNALS__);
  if (inTauri) return new TauriAppClient();
  return new DemoDesktopClient(
    browserClipboardWriter,
    browserClipboardReader,
    detectPlatform(),
  );
};
