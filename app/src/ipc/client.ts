import type { AppActivity, AppSettings, PlatformKind, UiSnapshot } from "../model";

export type Unsubscribe = () => void;

export interface AppClient {
  readonly platform: PlatformKind;
  getSnapshot(): Promise<UiSnapshot>;
  subscribe(listener: (snapshot: UiSnapshot) => void): Unsubscribe;
  createImportIntent(slotId: string, revision: number): Promise<string>;
  confirmImport(importId: string): Promise<void>;
  cancelImport(importId: string): Promise<void>;
  setPause(kind: "publish" | "subscribe", paused: boolean): Promise<void>;
  setSynchronizationPaused(paused: boolean): Promise<void>;
  setAppActivity(activity: "foreground" | "background"): Promise<void>;
  publishCurrentClipboard(): Promise<void>;
  updateSettings(settings: Partial<AppSettings>): Promise<void>;
  setGlobalShortcut(shortcut: string): Promise<void>;
  allowPairing(): Promise<void>;
  beginPairing(instanceId: string): Promise<void>;
  confirmPairing(pairingId: string, accepted: boolean): Promise<void>;
  setLocalDeviceName(deviceName: string): Promise<void>;
  setDeviceAlias(deviceId: string, localAlias: string | null): Promise<void>;
  setDeviceSyncEnabled(deviceId: string, enabled: boolean): Promise<void>;
  revokeDevice(deviceId: string): Promise<void>;
  createSyncGroup(input: { name: string; memberDeviceIds: string[]; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<string>;
  confirmGroupInvite(inviteId: string, accepted: boolean): Promise<void>;
  setGroupMemberDirection(groupId: string, deviceId: string, direction: "disabled" | "send_only" | "receive_only" | "bidirectional"): Promise<void>;
  removeGroupMember(groupId: string, deviceId: string): Promise<void>;
  updateGroupPolicy(input: { groupId: string; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<void>;
  leaveSyncGroup(groupId: string): Promise<void>;
  deleteSyncGroup(groupId: string): Promise<void>;
}

export type DesktopClient = AppClient;
