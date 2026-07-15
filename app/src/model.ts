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
  platform: "macos" | "windows" | "linux" | "android";
  online: boolean;
  pinned?: boolean;
  availability: SlotAvailability;
  preview: string;
  imagePreview?: string;
  fileNames?: string[];
  capturedAt: string;
  ageLabel: string;
  groups: string[];
  groupIds: string[];
  sequence: number;
  size: number;
  representations: ClipboardRepresentation[];
  blockedReason?: string;
  progress?: number;
}

export interface NearbyDevice {
  instanceId: string;
  deviceId: string;
  deviceName: string;
  platform: "macos" | "windows" | "linux" | "android" | "unknown";
  addresses: string[];
  port: number;
  lastSeenAt: string;
  paired: boolean;
}

export interface TrustedDevice {
  deviceId: string;
  deviceName: string;
  advertisedName: string;
  localAlias: string | null;
  platform: "macos" | "windows" | "linux" | "android" | "unknown";
  pairedAt: string;
  online: boolean;
  syncEnabled: boolean;
}

export interface PeerTelemetry {
  deviceId: string;
  connected: boolean;
  rttMs: number | null;
  uploadBps: number;
  downloadBps: number;
  recentUploadBps: number;
  recentDownloadBps: number;
  lossPercent: number;
  totalUploadedBytes: number;
  totalDownloadedBytes: number;
  connectedAt: string | null;
  lastActivityAt: string | null;
  reconnectCount: number;
  lastDisconnectReason: string | null;
  lastDisconnectCode: string | null;
  lastDisconnectedAt: string | null;
  lastDisconnectPlanned: boolean;
  unexpectedDisconnectCount: number;
}

export interface TransferTelemetry {
  id: string;
  attemptId: number;
  deviceId: string;
  direction: "upload" | "download";
  kind: "text" | "url" | "html" | "image" | "files";
  totalBytes: number;
  transferredBytes: number;
  startedAt: string;
  completedAt: string | null;
  durationMs: number;
  networkDurationMs: number | null;
  confirmationDurationMs: number | null;
  remoteProcessingMs: number | null;
  speedBps: number;
  averageBps: number;
  status: "active" | "success" | "sent" | "failed";
  message: string | null;
}

export interface TelemetrySnapshot {
  sampledAt: string;
  peers: PeerTelemetry[];
  transfers: TransferTelemetry[];
}

export const EMPTY_TELEMETRY: TelemetrySnapshot = {
  sampledAt: "1970-01-01T00:00:00Z",
  peers: [],
  transfers: [],
};

export interface PendingPairing {
  pairingId: string;
  deviceId: string;
  deviceName: string;
  platform: "macos" | "windows" | "linux" | "android" | "unknown";
  sas: string;
  direction: "incoming" | "outgoing";
  expiresAt: string;
  status: "awaiting_confirmation" | "peer_confirmed" | "waiting_for_peer" | "waiting_for_peer_complete";
}

export type GroupMemberState = "invited" | "active" | "removed";
export type SyncDirection = "disabled" | "send_only" | "receive_only" | "bidirectional";

export interface GroupPolicy {
  allowText: boolean;
  allowImages: boolean;
  allowHtml: boolean;
  allowFiles: boolean;
  offlineTtlSeconds: number;
}

export interface GroupMember {
  deviceId: string;
  deviceName: string;
  platform: "macos" | "windows" | "linux" | "android" | "unknown";
  publicKey: string;
  certificate: string;
  joinedAt: string;
  state: GroupMemberState;
  direction: SyncDirection;
}

export interface SyncGroup {
  groupId: string;
  name: string;
  ownerDeviceId: string;
  revision: number;
  membershipEpoch: number;
  isOwner: boolean;
  policy: GroupPolicy;
  members: GroupMember[];
}

export interface PendingGroupInvite {
  inviteId: string;
  groupId: string;
  groupName: string;
  ownerDeviceId: string;
  ownerName: string;
  expiresAt: string;
}

export interface CurrentClipboard {
  source: "local" | "remote" | "unknown";
  sourceLabel: string;
  preview: string;
  imagePreview?: string;
  fileNames?: string[];
  types: string[];
  changedAt: string;
}

export interface ImportOperation {
  id: string;
  slotId: string;
  revision: number;
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
  globalShortcut: string;
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
  localDeviceName: string;
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
  nearbyDevices: NearbyDevice[];
  trustedDevices: TrustedDevice[];
  pendingPairings: PendingPairing[];
  pairingAllowedUntil: number | null;
  cachePersistent: boolean;
  syncGroups: SyncGroup[];
  pendingGroupInvites: PendingGroupInvite[];
  imports: ImportOperation[];
  settings: AppSettings;
}

export const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
};

export const formatRate = (bytesPerSecond: number): string => `${formatBytes(bytesPerSecond)}/s`;
