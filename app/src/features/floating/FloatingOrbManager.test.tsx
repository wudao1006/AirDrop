import { render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { DemoDesktopClient } from "../../ipc/demo-client";
import type { FloatingAdapter } from "./floating-adapter";
import { FLOATING_EVENTS } from "./floating-events";
import {
  FLOATING_SIDE_STORAGE_KEY,
  FLOATING_HORIZONTAL_STORAGE_KEY,
  FLOATING_VERTICAL_STORAGE_KEY,
  FloatingOrbManager,
  readFloatingPlacement,
} from "./FloatingOrbManager";

const setup = async (overrides: Partial<FloatingAdapter> = {}) => {
  const demo = new DemoDesktopClient(async () => undefined, async () => "text");
  const snapshot: UiSnapshot = await demo.getSnapshot();
  snapshot.settings.floatingOrbEnabled = true;
  const handlers = new Map<string, (payload: unknown) => void>();
  const adapter: FloatingAdapter = {
    supported: true,
    ensureOrb: vi.fn(async () => undefined),
    closeOrb: vi.fn(async () => undefined),
    showMain: vi.fn(async () => undefined),
    emit: vi.fn(async () => undefined),
    listen: vi.fn(async (event, handler) => {
      handlers.set(event, handler as (payload: unknown) => void);
      return () => handlers.delete(event);
    }),
    onOrbMoved: vi.fn(async () => () => undefined),
    getOrbBounds: vi.fn(async () => ({ x: 928, y: 100, width: 72, height: 68 })),
    getOrbWorkArea: vi.fn(async () => ({ x: 0, y: 0, width: 1000, height: 700 })),
    setOrbBounds: vi.fn(async () => undefined),
    ...overrides,
  };
  return { demo, snapshot, handlers, adapter };
};

describe("FloatingOrbManager", () => {
  it("defaults an absent stored vertical fraction to the middle", () => {
    localStorage.removeItem(FLOATING_SIDE_STORAGE_KEY);
    localStorage.removeItem(FLOATING_HORIZONTAL_STORAGE_KEY);
    localStorage.removeItem(FLOATING_VERTICAL_STORAGE_KEY);
    expect(readFloatingPlacement()).toEqual({ side: "right", horizontalFraction: 1, verticalFraction: 0.5 });
  });

  it("does not create a programmatic move loop when dragged bounds are already valid", async () => {
    let moved: (() => void) | undefined;
    const { demo, snapshot, adapter } = await setup({
      onOrbMoved: vi.fn(async (handler) => { moved = handler; return () => undefined; }),
    });
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={vi.fn()} adapter={adapter} />);
    await waitFor(() => expect(moved).toBeTypeOf("function"));
    vi.mocked(adapter.setOrbBounds).mockClear();
    moved?.();
    await new Promise((resolve) => window.setTimeout(resolve, 220));
    expect(adapter.setOrbBounds).not.toHaveBeenCalled();
  });

  it("broadcasts state only after the ready handshake and dispatches actions", async () => {
    const { demo, snapshot, handlers, adapter } = await setup();
    const setPage = vi.fn();
    const pause = vi.spyOn(demo, "setSynchronizationPaused");
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={setPage} onError={vi.fn()} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    expect(adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.state, expect.anything());

    handlers.get(FLOATING_EVENTS.ready)?.({ protocolVersion: 1 });
    await waitFor(() => expect(adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.state, expect.objectContaining({ publishPaused: false })));

    handlers.get(FLOATING_EVENTS.action)?.({ action: "open-clipboard" });
    await waitFor(() => expect(setPage).toHaveBeenCalledWith("clipboard"));
    expect(adapter.showMain).toHaveBeenCalled();

    handlers.get(FLOATING_EVENTS.action)?.({ action: "toggle-sync" });
    await waitFor(() => expect(pause).toHaveBeenCalledWith(true));

    const createImport = vi.spyOn(demo, "createImportIntent");
    const confirmImport = vi.spyOn(demo, "confirmImport");
    handlers.get(FLOATING_EVENTS.action)?.({ action: "use-slot", slotId: "macbook-slot", revision: 7 });
    await waitFor(() => expect(createImport).toHaveBeenCalledWith("macbook-slot", 7));
    await waitFor(() => expect(setPage).toHaveBeenCalledWith("clipboard"));
    expect(adapter.showMain).toHaveBeenCalled();
    expect(confirmImport).not.toHaveBeenCalled();
  });

  it("acknowledges a clamped, side-aware layout transaction", async () => {
    const { demo, snapshot, handlers, adapter } = await setup();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={vi.fn()} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    handlers.get(FLOATING_EVENTS.layout)?.({ requestId: "layout-1", expanded: true });
    await waitFor(() => expect(adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layoutState, expect.objectContaining({
      requestId: "layout-1",
      expanded: true,
      success: true,
      side: "right",
      bounds: { x: 644, y: 100, width: 356, height: 420 },
      anchor: expect.objectContaining({ x: expect.any(Number), y: expect.any(Number) }),
    })));
    const payload = vi.mocked(adapter.emit).mock.calls.find(([event]) => event === FLOATING_EVENTS.layoutState)?.[1] as { anchor?: { x: number; y: number } };
    expect(payload.anchor?.x).toBeCloseTo(320 / 356);
    expect(payload.anchor?.y).toBeCloseTo(34 / 420);
  });

  it("always emits a failure ack when layout geometry is unavailable", async () => {
    const { demo, snapshot, handlers, adapter } = await setup({
      getOrbBounds: vi.fn(async () => { throw new Error("geometry unsupported"); }),
    });
    const onError = vi.fn();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={onError} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    handlers.get(FLOATING_EVENTS.layout)?.({ requestId: "layout-failed", expanded: true });
    await waitFor(() => expect(adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layoutState, {
      requestId: "layout-failed",
      expanded: true,
      success: false,
      message: "geometry unsupported",
    }));
    expect(onError).toHaveBeenCalledWith("geometry unsupported");
  });

  it("disables and closes immediately for hide-orb", async () => {
    const { demo, snapshot, handlers, adapter } = await setup();
    const update = vi.spyOn(demo, "updateSettings");
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={vi.fn()} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    handlers.get(FLOATING_EVENTS.action)?.({ action: "hide-orb" });
    await waitFor(() => expect(update).toHaveBeenCalledWith({ floatingOrbEnabled: false }));
    await waitFor(() => expect(adapter.closeOrb).toHaveBeenCalled());
  });

  it("reports action errors without disabling the orb", async () => {
    const { demo, snapshot, handlers, adapter } = await setup();
    vi.spyOn(demo, "publishCurrentClipboard").mockRejectedValue(new Error("clipboard denied"));
    const update = vi.spyOn(demo, "updateSettings");
    const onError = vi.fn();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={onError} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    handlers.get(FLOATING_EVENTS.action)?.({ action: "publish-current" });
    await waitFor(() => expect(onError).toHaveBeenCalledWith("clipboard denied"));
    expect(update).not.toHaveBeenCalledWith({ floatingOrbEnabled: false });
  });

  it("rolls the setting back when creation fails", async () => {
    const { demo, snapshot, adapter } = await setup({ ensureOrb: vi.fn(async () => { throw new Error("create denied"); }) });
    const update = vi.spyOn(demo, "updateSettings");
    const onError = vi.fn();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={onError} adapter={adapter} />);
    await waitFor(() => expect(update).toHaveBeenCalledWith({ floatingOrbEnabled: false }));
    expect(onError).toHaveBeenCalledWith("create denied");
  });

  it("keeps the orb enabled when optional move or geometry APIs are unsupported", async () => {
    const { demo, snapshot, adapter } = await setup({
      onOrbMoved: vi.fn(async () => { throw new Error("move events unsupported"); }),
      getOrbWorkArea: vi.fn(async () => { throw new Error("monitor unsupported"); }),
    });
    const update = vi.spyOn(demo, "updateSettings");
    const onError = vi.fn();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={onError} adapter={adapter} />);
    await waitFor(() => expect(adapter.ensureOrb).toHaveBeenCalled());
    await waitFor(() => expect(onError).toHaveBeenCalledWith("monitor unsupported"));
    expect(onError).toHaveBeenCalledWith("move events unsupported");
    expect(update).not.toHaveBeenCalledWith({ floatingOrbEnabled: false });
    expect(adapter.closeOrb).not.toHaveBeenCalled();
  });

  it("does not recreate after unmount while listener registration is pending", async () => {
    const { demo, snapshot, adapter } = await setup();
    let resolveListen!: (unlisten: () => void) => void;
    const unlisten = vi.fn();
    adapter.listen = vi.fn(() => new Promise((resolve) => { resolveListen = resolve; })) as FloatingAdapter["listen"];
    const view = render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={vi.fn()} adapter={adapter} />);
    view.unmount();
    resolveListen(unlisten);
    await waitFor(() => expect(unlisten).toHaveBeenCalled());
    expect(adapter.ensureOrb).not.toHaveBeenCalled();
  });

  it("rolls back already registered listeners when a later registration fails", async () => {
    const { demo, snapshot, adapter } = await setup();
    const firstUnlisten = vi.fn();
    adapter.listen = vi.fn()
      .mockResolvedValueOnce(firstUnlisten)
      .mockRejectedValueOnce(new Error("listen denied"));
    const onError = vi.fn();
    render(<FloatingOrbManager client={demo} snapshot={snapshot} setPage={vi.fn()} onError={onError} adapter={adapter} />);
    await waitFor(() => expect(onError).toHaveBeenCalledWith("listen denied"));
    expect(firstUnlisten).toHaveBeenCalledOnce();
    expect(adapter.ensureOrb).not.toHaveBeenCalled();
  });

  it("does nothing for Android even when the setting is present", async () => {
    const { snapshot, adapter } = await setup();
    const client = { platform: "android" } as DesktopClient;
    render(<FloatingOrbManager client={client} snapshot={{ ...snapshot, platform: "android" }} setPage={vi.fn()} onError={vi.fn()} adapter={adapter} />);
    await Promise.resolve();
    expect(adapter.ensureOrb).not.toHaveBeenCalled();
  });
});
