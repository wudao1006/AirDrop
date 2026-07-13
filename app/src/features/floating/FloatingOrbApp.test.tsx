import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FloatingEventPayloads, FloatingOrbStatePayload } from "./floating-events";
import { FLOATING_EVENTS } from "./floating-events";
import { FloatingOrbApp, type FloatingOrbWindowAdapter } from "./FloatingOrbApp";
import { AirDropSurface } from "../../main";

const liveState: FloatingOrbStatePayload = {
  publishPaused: false,
  subscribePaused: false,
  activity: "foreground_live",
  canReadClipboard: true,
  busy: false,
  slots: [],
  appearance: {
    theme: "light",
    accentColor: "#168fae",
    windowOpacity: 0.94,
    blurStrength: 30,
    glassSaturation: 1.3,
    cornerRadius: 22,
    highlightStrength: 0.28,
  },
};

const setup = () => {
  const handlers = new Map<string, (payload: never) => void>();
  const calls: string[] = [];
  const unlisteners: Array<ReturnType<typeof vi.fn>> = [];
  const adapter: FloatingOrbWindowAdapter = {
    emit: vi.fn(async (event) => { calls.push(`emit:${event}`); }),
    listen: vi.fn(async (event, handler) => {
      calls.push(`listen:${event}`);
      handlers.set(event, handler as (payload: never) => void);
      const unlisten = vi.fn(() => handlers.delete(event));
      unlisteners.push(unlisten);
      return unlisten;
    }),
    startDragging: vi.fn(async () => undefined),
  };
  const dispatch = <K extends keyof FloatingEventPayloads>(event: K, payload: FloatingEventPayloads[K]) =>
    handlers.get(event)?.(payload as never);
  return { adapter, calls, dispatch, unlisteners };
};

const openMenu = async (context: ReturnType<typeof setup>) => {
  fireEvent.contextMenu(screen.getByRole("button", { name: /AirDrop 悬浮球/ }));
  const layout = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
    event === FLOATING_EVENTS.layout && (payload as { expanded: boolean }).expanded,
  ).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
  context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: true });
  await screen.findByRole("region", { name: "AirDrop 快捷菜单" });
};

describe("FloatingOrbApp", () => {
  it("renders only the floating surface for the floating route", () => {
    const createClient = vi.fn();
    render(<AirDropSurface search="?surface=floating" createClient={createClient} />);
    expect(screen.getByRole("button", { name: /AirDrop 悬浮球/ })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "主导航" })).not.toBeInTheDocument();
    expect(createClient).not.toHaveBeenCalled();
  });

  it("installs all listeners before announcing ready", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, { protocolVersion: 1 }));
    expect(context.calls).toEqual([
      `listen:${FLOATING_EVENTS.state}`,
      `listen:${FLOATING_EVENTS.layoutState}`,
      `listen:${FLOATING_EVENTS.openMenu}`,
      `emit:${FLOATING_EVENTS.ready}`,
    ]);
  });

  it("uses the whole orb for dragging and opens only from the context menu", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    const orb = screen.getByRole("button", { name: /AirDrop 悬浮球/ });
    fireEvent.pointerDown(orb, { button: 0 });
    expect(context.adapter.startDragging).toHaveBeenCalledOnce();
    fireEvent.click(orb);
    expect(context.adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.anything());
    fireEvent.contextMenu(orb);
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: true, width: 356 }));
  });

  it("opens from the global-shortcut event after the matching layout ack", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.openMenu, {});
    const layout = vi.mocked(context.adapter.emit).mock.calls.find(([event]) => event === FLOATING_EVENTS.layout)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    expect(screen.queryByRole("region", { name: "AirDrop 快捷菜单" })).not.toBeInTheDocument();
    context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: true });
    expect(await screen.findByRole("region", { name: "AirDrop 快捷菜单" })).toBeInTheDocument();
  });

  it("shows remote content and emits a precise quick-use action", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, {
      ...liveState,
      slots: [{
        id: "slot-1",
        revision: 8,
        deviceName: "工作电脑",
        platform: "windows",
        kind: "files",
        preview: "2 个文件",
        fileNames: ["完整文件名设计稿.fig", "资料目录"],
        ageLabel: "刚刚",
        available: true,
      }],
    });
    await openMenu(context);
    expect(screen.getByText("完整文件名设计稿.fig · 资料目录")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "使用" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "use-slot", slotId: "slot-1", revision: 8 });
  });

  it("uses paused state for quick actions", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, { ...liveState, publishPaused: true, subscribePaused: true });
    await openMenu(context);
    fireEvent.click(screen.getByRole("button", { name: "恢复" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "toggle-sync" });
    fireEvent.click(screen.getByRole("button", { name: "隐藏" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "hide-orb" });
  });

  it("removes all native listeners on unmount", async () => {
    const context = setup();
    const view = render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    view.unmount();
    expect(context.unlisteners).toHaveLength(3);
    context.unlisteners.forEach((unlisten) => expect(unlisten).toHaveBeenCalledOnce());
  });
});
