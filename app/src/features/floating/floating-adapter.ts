import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, currentMonitor } from "@tauri-apps/api/window";
import type { Event as TauriEvent, UnlistenFn } from "@tauri-apps/api/event";
import type { Monitor } from "@tauri-apps/api/window";
import type { FloatingEventPayloads } from "./floating-events";
import { FLOATING_ORB_LABEL } from "./floating-events";
import type { FloatingRect } from "./floating-geometry";
import { COLLAPSED_ORB_SIZE } from "./floating-geometry";

type EventHandler<T> = (payload: T) => void;

export interface FloatingNativeWindow {
  label: string;
  once<T>(event: string, handler: (event: TauriEvent<T>) => void): Promise<UnlistenFn>;
  close(): Promise<void>;
  show(): Promise<void>;
  unminimize(): Promise<void>;
  setFocus(): Promise<void>;
  outerPosition(): Promise<{ x: number; y: number }>;
  outerSize(): Promise<{ width: number; height: number }>;
  scaleFactor(): Promise<number>;
  setPosition(position: LogicalPosition): Promise<void>;
  setSize(size: LogicalSize): Promise<void>;
  onMoved(handler: (event: TauriEvent<{ x: number; y: number }>) => void): Promise<UnlistenFn>;
}

export interface FloatingTauriBoundary {
  getWindow(label: string): Promise<FloatingNativeWindow | null>;
  createWindow(label: string, options: Record<string, unknown>): FloatingNativeWindow;
  emitTo<T>(label: string, event: string, payload: T): Promise<void>;
  listen<T>(event: string, handler: (event: TauriEvent<T>) => void): Promise<UnlistenFn>;
  currentMonitor(): Promise<Monitor | null>;
  availableMonitors(): Promise<Monitor[]>;
}

export interface FloatingAdapter {
  readonly supported: boolean;
  ensureOrb(): Promise<void>;
  closeOrb(): Promise<void>;
  showMain(): Promise<void>;
  emit<K extends keyof FloatingEventPayloads>(event: K, payload: FloatingEventPayloads[K]): Promise<void>;
  listen<K extends keyof FloatingEventPayloads>(event: K, handler: EventHandler<FloatingEventPayloads[K]>): Promise<UnlistenFn>;
  onOrbMoved(handler: () => void): Promise<UnlistenFn>;
  getOrbBounds(): Promise<FloatingRect>;
  getOrbWorkArea(): Promise<FloatingRect>;
  setOrbBounds(bounds: FloatingRect): Promise<void>;
}

const browserAdapter: FloatingAdapter = {
  supported: false,
  ensureOrb: async () => undefined,
  closeOrb: async () => undefined,
  showMain: async () => undefined,
  emit: async () => undefined,
  listen: async () => () => undefined,
  onOrbMoved: async () => () => undefined,
  getOrbBounds: async () => ({ x: 0, y: 0, ...COLLAPSED_ORB_SIZE }),
  getOrbWorkArea: async () => ({ x: 0, y: 0, ...COLLAPSED_ORB_SIZE }),
  setOrbBounds: async () => undefined,
};

export const createDefaultTauriBoundary = (): FloatingTauriBoundary => ({
  getWindow: (label) => WebviewWindow.getByLabel(label) as Promise<FloatingNativeWindow | null>,
  createWindow: (label, options) => new WebviewWindow(label, options) as FloatingNativeWindow,
  emitTo: (label, event, payload) => emitTo(label, event, payload),
  listen,
  currentMonitor,
  availableMonitors,
});

const logicalMonitorRect = (monitor: Monitor): FloatingRect => ({
  x: monitor.workArea.position.x / monitor.scaleFactor,
  y: monitor.workArea.position.y / monitor.scaleFactor,
  width: monitor.workArea.size.width / monitor.scaleFactor,
  height: monitor.workArea.size.height / monitor.scaleFactor,
});

const physicalMonitorRect = (monitor: Monitor): FloatingRect => ({
  x: monitor.position.x,
  y: monitor.position.y,
  width: monitor.size.width,
  height: monitor.size.height,
});

const containsCenter = (area: FloatingRect, bounds: FloatingRect): boolean => {
  const x = bounds.x + bounds.width / 2;
  const y = bounds.y + bounds.height / 2;
  return x >= area.x && x <= area.x + area.width && y >= area.y && y <= area.y + area.height;
};

export class TauriFloatingAdapter implements FloatingAdapter {
  readonly supported = true;
  private orb: FloatingNativeWindow | null = null;

  constructor(private readonly tauri: FloatingTauriBoundary) {}

  async ensureOrb(): Promise<void> {
    const existing = await this.tauri.getWindow(FLOATING_ORB_LABEL);
    if (existing) {
      this.orb = existing;
      await existing.show();
      await existing.setFocus();
      return;
    }

    const orb = this.tauri.createWindow(FLOATING_ORB_LABEL, {
      url: "?surface=floating",
      transparent: true,
      decorations: false,
      alwaysOnTop: true,
      skipTaskbar: true,
      resizable: false,
      width: COLLAPSED_ORB_SIZE.width,
      height: COLLAPSED_ORB_SIZE.height,
      visible: false,
    });

    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const unlisteners: UnlistenFn[] = [];
      const finish = (callback: () => void) => {
        if (settled) return;
        settled = true;
        unlisteners.forEach((unlisten) => unlisten());
        callback();
      };
      void orb.once("tauri://created", () => finish(resolve)).then((unlisten) => {
        if (settled) unlisten(); else unlisteners.push(unlisten);
      }).catch((error) => finish(() => reject(error)));
      void orb.once<unknown>("tauri://error", (event) => finish(() => reject(new Error(`无法创建悬浮球窗口：${String(event.payload)}`)))).then((unlisten) => {
        if (settled) unlisten(); else unlisteners.push(unlisten);
      }).catch((error) => finish(() => reject(error)));
    });
    this.orb = orb;
    await orb.show();
  }

  async closeOrb(): Promise<void> {
    const orb = this.orb ?? await this.tauri.getWindow(FLOATING_ORB_LABEL);
    this.orb = null;
    if (orb) await orb.close();
  }

  async showMain(): Promise<void> {
    const main = await this.tauri.getWindow("main");
    if (!main) throw new Error("主窗口不存在");
    await main.show();
    await main.unminimize();
    await main.setFocus();
  }

  emit<K extends keyof FloatingEventPayloads>(event: K, payload: FloatingEventPayloads[K]): Promise<void> {
    return this.tauri.emitTo(FLOATING_ORB_LABEL, event, payload);
  }

  async listen<K extends keyof FloatingEventPayloads>(event: K, handler: EventHandler<FloatingEventPayloads[K]>): Promise<UnlistenFn> {
    return this.tauri.listen<FloatingEventPayloads[K]>(event, ({ payload }) => handler(payload));
  }

  async onOrbMoved(handler: () => void): Promise<UnlistenFn> {
    const orb = await this.requireOrb();
    return orb.onMoved(handler);
  }

  async getOrbBounds(): Promise<FloatingRect> {
    const { position, size, scaleFactor } = await this.getOrbPhysicalMetrics();
    return {
      x: position.x / scaleFactor,
      y: position.y / scaleFactor,
      width: size.width / scaleFactor,
      height: size.height / scaleFactor,
    };
  }

  async getOrbWorkArea(): Promise<FloatingRect> {
    const { position, size, scaleFactor } = await this.getOrbPhysicalMetrics();
    const physicalBounds = { x: position.x, y: position.y, width: size.width, height: size.height };
    const monitors = await this.tauri.availableMonitors().catch(() => []);
    const matching = monitors.find((monitor) => containsCenter(physicalMonitorRect(monitor), physicalBounds));
    const monitor = matching ?? await this.tauri.currentMonitor().catch(() => null);
    return monitor ? logicalMonitorRect(monitor) : {
      x: position.x / scaleFactor,
      y: position.y / scaleFactor,
      width: size.width / scaleFactor,
      height: size.height / scaleFactor,
    };
  }

  async setOrbBounds(bounds: FloatingRect): Promise<void> {
    const orb = await this.requireOrb();
    // Keep the anchored edge stable by moving before resizing when growing left.
    await orb.setPosition(new LogicalPosition(Math.round(bounds.x), Math.round(bounds.y)));
    await orb.setSize(new LogicalSize(Math.round(bounds.width), Math.round(bounds.height)));
  }

  private async requireOrb(): Promise<FloatingNativeWindow> {
    const orb = this.orb ?? await this.tauri.getWindow(FLOATING_ORB_LABEL);
    if (!orb) throw new Error("悬浮球窗口尚未创建");
    this.orb = orb;
    return orb;
  }

  private async getOrbPhysicalMetrics(): Promise<{
    position: { x: number; y: number };
    size: { width: number; height: number };
    scaleFactor: number;
  }> {
    const orb = await this.requireOrb();
    const [position, size, scaleFactor] = await Promise.all([orb.outerPosition(), orb.outerSize(), orb.scaleFactor()]);
    return { position, size, scaleFactor };
  }
}

export const createFloatingAdapter = (): FloatingAdapter => {
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) return browserAdapter;
  return new TauriFloatingAdapter(createDefaultTauriBoundary());
};

export class FloatingOrbReconciler {
  private desired = false;
  private generation = 0;
  private queue: Promise<void> = Promise.resolve();

  constructor(private readonly adapter: FloatingAdapter) {}

  reconcile(enabled: boolean): Promise<void> {
    this.desired = enabled;
    const generation = ++this.generation;
    this.queue = this.queue.catch(() => undefined).then(async () => {
      if (!this.adapter.supported) return;
      if (!this.desired || generation !== this.generation) {
        if (!this.desired) await this.adapter.closeOrb();
        return;
      }
      await this.adapter.ensureOrb();
      if (!this.desired || generation !== this.generation) await this.adapter.closeOrb();
    });
    return this.queue;
  }
}
