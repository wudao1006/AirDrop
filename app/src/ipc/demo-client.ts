import type { DesktopClient, Unsubscribe } from "./client";
import type { AppSettings, DeviceSlot, ImportOperation, PlatformKind, TelemetrySnapshot, UiSnapshot } from "../model";
import {
  DEFAULT_APPEARANCE_SETTINGS,
  extractAppearanceSettings,
  loadAppearanceSettings,
  saveAppearanceSettings,
} from "../features/settings/appearance";

const now = new Date().toISOString();

const initialTelemetry: TelemetrySnapshot = {
  sampledAt: now,
  peers: [{
    deviceId: "device-macbook",
    connected: true,
    rttMs: 18,
    uploadBps: 42_000,
    downloadBps: 118_000,
    recentUploadBps: 38_000,
    recentDownloadBps: 104_000,
    lossPercent: 0.12,
    totalUploadedBytes: 8_420_000,
    totalDownloadedBytes: 18_870_000,
    connectedAt: new Date(Date.now() - 18 * 60_000).toISOString(),
    lastActivityAt: now,
    reconnectCount: 1,
    lastDisconnectReason: null,
    lastDisconnectCode: null,
    lastDisconnectedAt: null,
    lastDisconnectPlanned: false,
    unexpectedDisconnectCount: 0,
  }],
  transfers: [{
    id: "demo-transfer-1",
    attemptId: 1,
    deviceId: "device-macbook",
    direction: "download",
    kind: "text",
    totalBytes: 94,
    transferredBytes: 94,
    startedAt: now,
    completedAt: now,
    durationMs: 286,
    networkDurationMs: 112,
    confirmationDurationMs: 174,
    remoteProcessingMs: 8,
    speedBps: 0,
    averageBps: 329,
    status: "success",
    message: "已写入设备槽位",
  }],
};

const initialSlots: DeviceSlot[] = [
  {
    id: "macbook-slot",
    revision: 7,
    deviceId: "device-macbook",
    deviceName: "MacBook Pro",
    platform: "macos",
    online: true,
    pinned: true,
    availability: "ready",
    preview: "这段文字来自 MacBook，可以选择后写入当前设备。",
    capturedAt: now,
    ageLabel: "8 秒前",
    groups: ["个人设备", "工作组"],
    groupIds: ["demo-personal", "demo-work"],
    sequence: 1842,
    size: 94,
    representations: [
      { id: "mac-text", kind: "text", label: "纯文本", mime: "text/plain", size: 94, status: "ready", enabled: true },
      { id: "mac-html", kind: "html", label: "HTML", mime: "text/html", size: 182, status: "ready", enabled: true },
    ],
  },
  {
    id: "work-pc-slot",
    revision: 4,
    deviceId: "device-work-pc",
    deviceName: "Work PC",
    platform: "windows",
    online: true,
    availability: "metadata_only",
    preview: "PNG 图片 · 2560 × 1440",
    capturedAt: now,
    ageLabel: "1 分钟前",
    groups: ["工作组"],
    groupIds: ["demo-work"],
    sequence: 907,
    size: 3_355_443,
    representations: [
      { id: "pc-image", kind: "image", label: "PNG 图片", mime: "image/png", size: 3_355_443, status: "fetching", enabled: true },
    ],
  },
  {
    id: "linux-slot",
    revision: 9,
    deviceId: "device-linux",
    deviceName: "Studio Linux",
    platform: "linux",
    online: false,
    availability: "stale",
    preview: "https://tauri.app/start/",
    capturedAt: now,
    ageLabel: "4 分钟前",
    groups: ["个人设备"],
    groupIds: ["demo-personal"],
    sequence: 331,
    size: 24,
    representations: [
      { id: "linux-url", kind: "url", label: "链接", mime: "text/uri-list", size: 24, status: "ready", enabled: true },
      { id: "linux-text", kind: "text", label: "纯文本", mime: "text/plain", size: 24, status: "ready", enabled: true },
    ],
  },
  {
    id: "tablet-slot",
    revision: 2,
    deviceId: "device-tablet",
    deviceName: "Lab Device",
    platform: "linux",
    online: true,
    availability: "blocked",
    preview: "私有应用格式",
    capturedAt: now,
    ageLabel: "12 分钟前",
    groups: ["实验室"],
    groupIds: ["demo-lab"],
    sequence: 52,
    size: 4_096,
    blockedReason: "当前同步组未允许此私有格式",
    representations: [
      { id: "lab-private", kind: "private", label: "应用私有格式", mime: "application/x-private", size: 4_096, status: "blocked", enabled: false },
    ],
  },
];

const initialSnapshot: UiSnapshot = {
  revision: 1,
  platform: "desktop",
  localDeviceName: "我的设备",
  activity: "foreground_live",
  lastSynchronizedAt: now,
  clipboardCapability: {
    canReadText: true,
    canWriteText: true,
    foregroundCapture: true,
  },
  demoMode: true,
  daemonConnected: false,
  publishPaused: false,
  subscribePaused: false,
  currentClipboard: {
    source: "local",
    sourceLabel: "来自本机应用",
    preview: "当前本机剪贴板不会被远端更新自动覆盖",
    types: ["纯文本"],
    changedAt: now,
  },
  lastPublishedPreview: "本机最近发布：一段更早的本地复制内容",
  slots: initialSlots,
  nearbyDevices: [],
  trustedDevices: [],
  pendingPairings: [],
  pairingAllowedUntil: null,
  cachePersistent: false,
  syncGroups: [],
  pendingGroupInvites: [],
  imports: [],
  settings: {
    ...DEFAULT_APPEARANCE_SETTINGS,
    previewText: true,
    previewImages: false,
    previewFileNames: false,
    allowText: true,
    allowHtml: true,
    allowImages: true,
    allowUrls: true,
    allowFiles: false,
    allowPrivate: false,
    globalShortcut: "Ctrl+Alt+KeyZ",
  },
};

type ClipboardWriter = (text: string) => Promise<void>;
type ClipboardReader = () => Promise<string>;

export class DemoDesktopClient implements DesktopClient {
  readonly platform: PlatformKind;
  private snapshot = structuredClone(initialSnapshot);
  private telemetry = structuredClone(initialTelemetry);
  private listeners = new Set<(snapshot: UiSnapshot) => void>();
  private telemetryListeners = new Set<(snapshot: TelemetrySnapshot) => void>();
  private payloads = new Map<string, string>([
    ["macbook-slot", "这段文字来自 MacBook，可以选择后写入当前设备。"],
    ["linux-slot", "https://tauri.app/start/"],
  ]);
  private timers = new Map<string, number>();

  constructor(
    private readonly writeClipboard: ClipboardWriter,
    private readonly readClipboard: ClipboardReader = async () => "",
    platform: PlatformKind = "desktop",
  ) {
    this.platform = platform;
    this.snapshot.platform = platform;
    this.snapshot.settings = { ...this.snapshot.settings, ...loadAppearanceSettings() };
  }

  async getSnapshot(): Promise<UiSnapshot> {
    return structuredClone(this.snapshot);
  }

  subscribe(listener: (snapshot: UiSnapshot) => void): Unsubscribe {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async getTelemetry(): Promise<TelemetrySnapshot> {
    return structuredClone(this.telemetry);
  }

  subscribeTelemetry(listener: (snapshot: TelemetrySnapshot) => void): Unsubscribe {
    this.telemetryListeners.add(listener);
    return () => this.telemetryListeners.delete(listener);
  }

  async setTelemetryObserving(_observing: boolean): Promise<void> {}

  async copyDiagnosticReport(report: string): Promise<void> {
    await this.writeClipboard(report);
  }

  async useSlot(slotId: string, revision: number): Promise<void> {
    const importId = await this.createImportIntent(slotId, revision);
    await this.confirmImport(importId);
  }

  async createImportIntent(slotId: string, revision: number): Promise<string> {
    if (this.platform === "android" && this.snapshot.activity !== "foreground_live") {
      throw new Error("回到前台并完成重连后才能选择设备剪贴板");
    }
    const slot = this.snapshot.slots.find((candidate) => candidate.id === slotId);
    if (!slot || slot.revision !== revision) throw new Error("设备槽位已经更新，请重新选择");
    if (["expired", "blocked", "protocol_conflict"].includes(slot.availability)) {
      throw new Error(slot.blockedReason ?? "此设备槽位当前不可用");
    }

    const importId = crypto.randomUUID();
    const immediate = slot.availability === "ready" || slot.availability === "stale";
    const operation: ImportOperation = {
      id: importId,
      slotId,
      revision,
      deviceName: slot.deviceName,
      sourceSummary: `${slot.deviceName} · ${slot.groups.join(" + ")} · #${slot.sequence}`,
      status: immediate ? "awaiting_confirmation" : "fetching",
      progress: immediate ? 100 : 12,
      message: immediate ? "内容已就绪，请确认写入本机剪贴板" : "正在获取并校验内容",
    };
    this.snapshot.imports = [operation, ...this.snapshot.imports];
    this.bump();

    if (!immediate) {
      const timer = window.setInterval(() => {
        const current = this.snapshot.imports.find((item) => item.id === importId);
        if (!current || current.status !== "fetching") return;
        current.progress = Math.min(100, current.progress + 18);
        if (current.progress >= 100) {
          current.status = "awaiting_confirmation";
          current.message = "演示内容已就绪；真实网络 Daemon 尚未接入";
          window.clearInterval(timer);
          this.timers.delete(importId);
        }
        this.bump();
      }, 320);
      this.timers.set(importId, timer);
    }
    return importId;
  }

  async confirmImport(importId: string): Promise<void> {
    if (this.platform === "android" && this.snapshot.activity !== "foreground_live") {
      throw new Error("回到前台并重新校验授权后才能写入剪贴板");
    }
    const operation = this.snapshot.imports.find((item) => item.id === importId);
    if (!operation || operation.status !== "awaiting_confirmation") throw new Error("Import 尚未准备好");
    const slot = this.snapshot.slots.find((candidate) => candidate.id === operation.slotId);
    const payload = this.payloads.get(operation.slotId);
    if (!slot || !payload) {
      operation.status = "unavailable";
      operation.message = "演示槽位没有可写入的文本正文";
      this.bump();
      throw new Error(operation.message);
    }

    operation.status = "committing";
    operation.message = "正在写入本机剪贴板";
    this.bump();
    try {
      await this.writeClipboard(payload);
      operation.status = "imported";
      operation.message = "已取入本机剪贴板";
      this.snapshot.currentClipboard = {
        source: "remote",
        sourceLabel: `取自 ${slot.deviceName}`,
        preview: payload,
        types: slot.representations.map((item) => item.label),
        changedAt: new Date().toISOString(),
      };
      this.bump();
    } catch (error) {
      operation.status = "failed";
      operation.message = error instanceof Error ? error.message : "无法写入本机剪贴板";
      this.bump();
      throw error;
    }
  }

  async cancelImport(importId: string): Promise<void> {
    const timer = this.timers.get(importId);
    if (timer) window.clearInterval(timer);
    this.timers.delete(importId);
    this.snapshot.imports = this.snapshot.imports.filter((item) => item.id !== importId);
    this.bump();
  }

  async setPause(kind: "publish" | "subscribe", paused: boolean): Promise<void> {
    if (kind === "publish") this.snapshot.publishPaused = paused;
    else this.snapshot.subscribePaused = paused;
    this.bump();
  }

  async setSynchronizationPaused(paused: boolean): Promise<void> {
    this.snapshot.publishPaused = paused;
    this.snapshot.subscribePaused = paused;
    this.bump();
  }

  async setAppActivity(activity: "foreground" | "background"): Promise<void> {
    if (this.platform !== "android") return;
    if (activity === "background") {
      this.snapshot.activity = "suspended";
      this.bump();
      return;
    }

    this.snapshot.activity = "reconnecting";
    this.bump();
    await Promise.resolve();
    this.snapshot.activity = "foreground_live";
    this.snapshot.lastSynchronizedAt = new Date().toISOString();
    this.bump();
  }

  async publishCurrentClipboard(): Promise<void> {
    if (this.platform === "android" && this.snapshot.activity !== "foreground_live") {
      throw new Error("回到前台并完成重连后才能读取当前剪贴板");
    }
    try {
      const text = await this.readClipboard();
      if (!text.trim()) throw new Error("当前文本剪贴板为空");
      this.snapshot.clipboardCapability = {
        ...this.snapshot.clipboardCapability,
        canReadText: true,
        foregroundCapture: true,
        limitation: undefined,
      };
      this.snapshot.currentClipboard = {
        source: "local",
        sourceLabel: "来自本机应用",
        preview: text,
        types: ["纯文本"],
        changedAt: new Date().toISOString(),
      };
      this.snapshot.lastPublishedPreview = `本机最近发布：${text.slice(0, 80)}`;
      this.bump();
    } catch (error) {
      const message = error instanceof Error ? error.message : "系统拒绝读取当前剪贴板";
      if (message !== "当前文本剪贴板为空") {
        this.snapshot.clipboardCapability = {
          ...this.snapshot.clipboardCapability,
          canReadText: false,
          foregroundCapture: false,
          limitation: message,
        };
        this.bump();
      }
      throw error;
    }
  }

  async updateSettings(settings: Partial<AppSettings>): Promise<void> {
    const mergedSettings = { ...this.snapshot.settings, ...settings };
    const appearance = extractAppearanceSettings(mergedSettings);
    this.snapshot.settings = { ...mergedSettings, ...appearance };
    saveAppearanceSettings(appearance);
    this.bump();
  }

  async setGlobalShortcut(shortcut: string): Promise<void> {
    this.snapshot.settings.globalShortcut = shortcut;
    this.bump();
  }

  async allowPairing(): Promise<void> {
    throw new Error("浏览器预览不能开放局域网配对，请打开 AirDrop 应用");
  }

  async beginPairing(_instanceId: string): Promise<void> {
    throw new Error("浏览器预览不能建立设备配对，请打开 AirDrop 应用");
  }

  async confirmPairing(_pairingId: string, _accepted: boolean): Promise<void> {
    throw new Error("浏览器预览没有真实配对会话");
  }

  async setLocalDeviceName(deviceName: string): Promise<void> {
    const normalized = deviceName.trim();
    if (!normalized) throw new Error("本机名称不能为空");
    this.snapshot.localDeviceName = normalized;
    this.bump();
  }

  async setDeviceAlias(deviceId: string, localAlias: string | null): Promise<void> {
    const device = this.snapshot.trustedDevices.find((item) => item.deviceId === deviceId);
    if (!device) throw new Error("可信设备不存在");
    const normalized = localAlias?.trim() || null;
    device.localAlias = normalized;
    device.deviceName = normalized ?? device.advertisedName;
    for (const slot of this.snapshot.slots.filter((item) => item.deviceId === deviceId)) {
      slot.deviceName = device.deviceName;
    }
    this.bump();
  }

  async setDeviceSyncEnabled(deviceId: string, enabled: boolean): Promise<void> {
    const device = this.snapshot.trustedDevices.find((item) => item.deviceId === deviceId);
    if (!device) throw new Error("可信设备不存在");
    device.syncEnabled = enabled;
    if (!enabled) device.online = false;
    this.bump();
  }

  async revokeDevice(deviceId: string): Promise<void> {
    this.snapshot.trustedDevices = this.snapshot.trustedDevices.filter((item) => item.deviceId !== deviceId);
    this.snapshot.slots = this.snapshot.slots.filter((item) => item.deviceId !== deviceId);
    this.bump();
  }

  async createSyncGroup(_input: { name: string; memberDeviceIds: string[]; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<string> {
    throw new Error("浏览器预览不能创建真实同步组，请打开 AirDrop 应用");
  }

  async confirmGroupInvite(_inviteId: string, _accepted: boolean): Promise<void> {
    throw new Error("浏览器预览没有真实同步组邀请");
  }

  async setGroupMemberDirection(_groupId: string, _deviceId: string, _direction: "disabled" | "send_only" | "receive_only" | "bidirectional"): Promise<void> {
    throw new Error("浏览器预览不能修改真实同步组");
  }

  async removeGroupMember(_groupId: string, _deviceId: string): Promise<void> {
    throw new Error("浏览器预览不能移除真实同步组成员");
  }

  async updateGroupPolicy(_input: { groupId: string; allowText: boolean; allowImages: boolean; allowHtml: boolean; allowFiles: boolean }): Promise<void> {
    throw new Error("浏览器预览不能修改真实同步组策略");
  }

  async leaveSyncGroup(_groupId: string): Promise<void> {
    throw new Error("浏览器预览不能退出真实同步组");
  }

  async deleteSyncGroup(_groupId: string): Promise<void> {
    throw new Error("浏览器预览不能结束真实同步组");
  }

  private bump(): void {
    this.snapshot.revision += 1;
    const copy = structuredClone(this.snapshot);
    this.listeners.forEach((listener) => listener(copy));
  }
}
