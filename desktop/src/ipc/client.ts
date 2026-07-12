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
}

export type DesktopClient = AppClient;
