export type PageId = "home" | "clipboard" | "devices" | "groups" | "transfers" | "settings";
export type PlatformKind = "desktop" | "android";
export type AppActivity = "foreground_live" | "reconnecting" | "suspended";

export interface ClipboardCapability {
  canReadText: boolean;
  canWriteText: boolean;
  foregroundCapture: boolean;
  limitation?: string;
}

export type SlotAvailability =
  | "metadata_only"
  | "partial"
  | "ready"
  | "stale"
  | "expired"
  | "blocked"
  | "protocol_conflict";

export type ImportStatus =
  | "fetching"
  | "awaiting_confirmation"
  | "committing"
  | "imported"
  | "unavailable"
  | "failed"
  | "failed_after_mutation";

export type RepresentationKind = "text" | "html" | "image" | "url" | "files" | "private";

export interface ClipboardRepresentation {
  id: string;
  kind: RepresentationKind;
  label: string;
  mime: string;
  size: number;
  status: "ready" | "fetching" | "blocked" | "unsupported";
  enabled: boolean;
}

export interface DeviceSlot {
  id: string;
  revision: number;
  deviceId: string;
  deviceName: string;
  platform: "macos" | "windows" | "linux";
  online: boolean;
  pinned?: boolean;
  availability: SlotAvailability;
  preview: string;
  capturedAt: string;
  ageLabel: string;
  groups: string[];
  sequence: number;
  size: number;
  representations: ClipboardRepresentation[];
  blockedReason?: string;
  progress?: number;
}

export interface CurrentClipboard {
  source: "local" | "remote" | "unknown";
  sourceLabel: string;
  preview: string;
  types: string[];
  changedAt: string;
}

export interface ImportOperation {
  id: string;
  slotId: string;
  deviceName: string;
  sourceSummary: string;
  status: ImportStatus;
  progress: number;
  message?: string;
}

export interface AppSettings {
  theme: "system" | "light" | "dark";
  accentColor: string;
  windowOpacity: number;
  blurStrength: number;
  glassSaturation: number;
  cornerRadius: number;
  highlightStrength: number;
  floatingOrbEnabled: boolean;
  previewText: boolean;
  previewImages: boolean;
  previewFileNames: boolean;
  allowText: boolean;
  allowHtml: boolean;
  allowImages: boolean;
  allowUrls: boolean;
  allowFiles: boolean;
  allowPrivate: boolean;
}

export interface UiSnapshot {
  revision: number;
  platform: PlatformKind;
  activity: AppActivity;
  lastSynchronizedAt: string;
  clipboardCapability: ClipboardCapability;
  demoMode: boolean;
  daemonConnected: boolean;
  publishPaused: boolean;
  subscribePaused: boolean;
  currentClipboard: CurrentClipboard;
  lastPublishedPreview: string;
  slots: DeviceSlot[];
  imports: ImportOperation[];
  settings: AppSettings;
}

export const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
};
