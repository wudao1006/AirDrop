import { describe, expect, it, vi } from "vitest";
import type { Event, UnlistenFn } from "@tauri-apps/api/event";
import type { Monitor } from "@tauri-apps/api/window";
import {
  FloatingOrbReconciler,
  TauriFloatingAdapter,
  createFloatingAdapter,
  type FloatingAdapter,
  type FloatingNativeWindow,
  type FloatingTauriBoundary,
} from "./floating-adapter";
import { FLOATING_EVENTS } from "./floating-events";

const nativeWindow = (created: "success" | "error" = "success"): FloatingNativeWindow => ({
  label: "floating-orb",
  once: vi.fn(async (event: string, handler: (event: Event<unknown>) => void): Promise<UnlistenFn> => {
    if ((created === "success" && event === "tauri://created") || (created === "error" && event === "tauri://error")) {
      queueMicrotask(() => handler({ payload: created === "error" ? "denied" : undefined } as Event<unknown>));
    }
    return () => undefined;
  }) as FloatingNativeWindow["once"],
  close: vi.fn(async () => undefined),
  show: vi.fn(async () => undefined),
  unminimize: vi.fn(async () => undefined),
  setFocus: vi.fn(async () => undefined),
  outerPosition: vi.fn(async () => ({ x: 0, y: 0 })),
  outerSize: vi.fn(async () => ({ width: 72, height: 68 })),
  scaleFactor: vi.fn(async () => 1),
  setPosition: vi.fn(async () => undefined),
  setSize: vi.fn(async () => undefined),
  onMoved: vi.fn(async () => () => undefined),
});

const boundary = (window: FloatingNativeWindow): FloatingTauriBoundary => ({
  getWindow: vi.fn(async () => null),
  createWindow: vi.fn(() => window),
  emitTo: vi.fn(async () => undefined),
  listen: vi.fn(async () => () => undefined),
  currentMonitor: vi.fn(async () => null),
  availableMonitors: vi.fn(async () => [] as Monitor[]),
});

describe("floating adapter", () => {
  it("uses the namespaced wire protocol event names", () => {
    expect(FLOATING_EVENTS).toEqual({
      ready: "airdrop://orb-ready",
      state: "airdrop://orb-state",
      action: "airdrop://orb-action",
      layout: "airdrop://orb-layout",
      layoutState: "airdrop://orb-layout-state",
    });
  });

  it("is a safe no-op outside Tauri", async () => {
    const adapter = createFloatingAdapter();
    expect(adapter.supported).toBe(false);
    await expect(adapter.ensureOrb()).resolves.toBeUndefined();
  });

  it("waits for tauri://created before reporting success", async () => {
    const window = nativeWindow();
    const tauri = boundary(window);
    const adapter = new TauriFloatingAdapter(tauri);
    await adapter.ensureOrb();
    expect(tauri.createWindow).toHaveBeenCalledWith("floating-orb", expect.objectContaining({ url: "?surface=floating", width: 72, height: 68, shadow: false, focus: false }));
    expect(window.show).toHaveBeenCalledOnce();
  });

  it("shows and focuses an existing orb instead of creating a duplicate", async () => {
    const window = nativeWindow();
    const tauri = boundary(window);
    vi.mocked(tauri.getWindow).mockResolvedValue(window);
    const adapter = new TauriFloatingAdapter(tauri);
    await adapter.ensureOrb();
    expect(tauri.createWindow).not.toHaveBeenCalled();
    expect(window.show).toHaveBeenCalledOnce();
    expect(window.setFocus).not.toHaveBeenCalled();
  });

  it("reports tauri://error creation failures", async () => {
    const adapter = new TauriFloatingAdapter(boundary(nativeWindow("error")));
    await expect(adapter.ensureOrb()).rejects.toThrow("denied");
  });

  it("cleans up a partially registered creation listener when the other registration fails", async () => {
    const cleanupCreated = vi.fn();
    const window = nativeWindow();
    vi.mocked(window.once).mockImplementation((event: string) => event === "tauri://created"
      ? Promise.resolve(cleanupCreated)
      : Promise.reject(new Error("event registration failed")));
    const adapter = new TauriFloatingAdapter(boundary(window));
    await expect(adapter.ensureOrb()).rejects.toThrow("event registration failed");
    expect(cleanupCreated).toHaveBeenCalledOnce();
  });

  it("does not leave a window alive after a rapid disable", async () => {
    let finish!: () => void;
    const adapter = {
      supported: true,
      ensureOrb: vi.fn(() => new Promise<void>((resolve) => { finish = resolve; })),
      closeOrb: vi.fn(async () => undefined),
    } as unknown as FloatingAdapter;
    const reconciler = new FloatingOrbReconciler(adapter);
    const enabling = reconciler.reconcile(true);
    await vi.waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    const disabling = reconciler.reconcile(false);
    finish();
    await Promise.all([enabling, disabling]);
    expect(adapter.closeOrb).toHaveBeenCalled();
  });

  it("coalesces duplicate enable requests before creation starts", async () => {
    const adapter = {
      supported: true,
      ensureOrb: vi.fn(async () => undefined),
      closeOrb: vi.fn(async () => undefined),
    } as unknown as FloatingAdapter;
    const reconciler = new FloatingOrbReconciler(adapter);
    await Promise.all([reconciler.reconcile(true), reconciler.reconcile(true)]);
    expect(adapter.ensureOrb).toHaveBeenCalledOnce();
  });

  it("falls back to current bounds when monitor APIs are unavailable", async () => {
    const window = nativeWindow();
    const tauri = boundary(window);
    vi.mocked(tauri.getWindow).mockResolvedValue(window);
    vi.mocked(tauri.availableMonitors).mockRejectedValue(new Error("unsupported"));
    vi.mocked(tauri.currentMonitor).mockRejectedValue(new Error("unsupported"));
    const adapter = new TauriFloatingAdapter(tauri);
    await adapter.ensureOrb();
    await expect(adapter.getOrbWorkArea()).resolves.toEqual({ x: 0, y: 0, width: 72, height: 68 });
  });

  it("selects a mixed-DPI monitor using physical coordinates", async () => {
    const window = nativeWindow();
    vi.mocked(window.outerPosition).mockResolvedValue({ x: 2100, y: 100 });
    vi.mocked(window.outerSize).mockResolvedValue({ width: 144, height: 136 });
    vi.mocked(window.scaleFactor).mockResolvedValue(2);
    const tauri = boundary(window);
    vi.mocked(tauri.getWindow).mockResolvedValue(window);
    vi.mocked(tauri.availableMonitors).mockResolvedValue([
      {
        name: "primary",
        scaleFactor: 1,
        position: { x: 0, y: 0 },
        size: { width: 1920, height: 1080 },
        workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      },
      {
        name: "hidpi",
        scaleFactor: 2,
        position: { x: 1920, y: 0 },
        size: { width: 2560, height: 1600 },
        workArea: { position: { x: 1920, y: 0 }, size: { width: 2560, height: 1520 } },
      },
    ] as Monitor[]);
    const adapter = new TauriFloatingAdapter(tauri);
    await adapter.ensureOrb();
    await expect(adapter.getOrbWorkArea()).resolves.toEqual({ x: 960, y: 0, width: 1280, height: 760 });
  });
});
