import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FloatingEventPayloads, FloatingOrbStatePayload } from "./floating-events";
import { FLOATING_EVENTS } from "./floating-events";
import { FloatingOrbApp, type FloatingOrbWindowAdapter } from "./FloatingOrbApp";
import type { FloatingContentDragAdapter } from "./floating-drag";
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
    expect(document.documentElement).toHaveClass("floating-surface");
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
      `listen:${FLOATING_EVENTS.actionResult}`,
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
        online: true,
        kind: "files",
        preview: "2 个文件",
        fileNames: ["完整文件名设计稿.fig", "资料目录"],
        ageLabel: "刚刚",
        available: true,
      }],
    });
    await openMenu(context);
    expect(screen.getByText("1 台设备 · 拖动内容可直接插入")).toBeInTheDocument();
    expect(screen.getByText("文件")).toBeInTheDocument();
    expect(screen.getByText("完整文件名设计稿.fig · 资料目录")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "使用" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, expect.objectContaining({ action: "use-slot", slotId: "slot-1", revision: 8, requestId: expect.any(String) }));
    const request = vi.mocked(context.adapter.emit).mock.calls.find(([event, payload]) => event === FLOATING_EVENTS.action && (payload as { action: string }).action === "use-slot")?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.action];
    expect(screen.getByRole("button", { name: "取入中" })).toBeDisabled();
    context.dispatch(FLOATING_EVENTS.actionResult, { requestId: request.requestId, success: true, message: "已写入本机剪贴板" });
    expect(await screen.findByRole("button", { name: "使用" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("已写入本机剪贴板");
  });

  it("starts a native content drag from the card body without using the clipboard action", async () => {
    const context = setup();
    const dragAdapter: FloatingContentDragAdapter = {
      supported: true,
      prepare: vi.fn(async () => ({ item: { data: "完整正文", types: ["text/plain"] } })),
      startFiles: vi.fn(async () => undefined),
    };
    render(<FloatingOrbApp adapter={context.adapter} dragAdapter={dragAdapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, {
      ...liveState,
      slots: [{
        id: "slot-text",
        revision: 12,
        deviceName: "书房电脑",
        platform: "linux",
        online: true,
        kind: "text",
        preview: "拖入目标编辑器",
        ageLabel: "刚刚",
        available: true,
      }],
    });
    await openMenu(context);
    const content = screen.getByText("拖入目标编辑器");
    await waitFor(() => expect(content.closest(".floating-device-copy")).toHaveAttribute("draggable", "true"));
    const dataTransfer = { setData: vi.fn(), setDragImage: vi.fn(), effectAllowed: "none" };
    fireEvent.dragStart(content, { dataTransfer });
    expect(dragAdapter.prepare).toHaveBeenCalledWith("slot-text", 12);
    expect(dataTransfer.setData).toHaveBeenCalledWith("text/plain", "完整正文");
    expect(dataTransfer.effectAllowed).toBe("copy");
    expect(context.adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.action, expect.objectContaining({ action: "use-slot" }));
  });

  it("uses the native file drag path for images and file bundles", async () => {
    const context = setup();
    const dragAdapter: FloatingContentDragAdapter = {
      supported: true,
      prepare: vi.fn(async () => ({ item: ["/tmp/report.pdf"], leaseId: "lease-1" })),
      startFiles: vi.fn(async (_request, _prepared, onEvent) => onEvent({ result: "Dropped" })),
    };
    render(<FloatingOrbApp adapter={context.adapter} dragAdapter={dragAdapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, {
      ...liveState,
      slots: [{
        id: "slot-files",
        revision: 4,
        deviceName: "工作电脑",
        platform: "windows",
        online: true,
        kind: "files",
        preview: "1 个文件",
        fileNames: ["report.pdf"],
        ageLabel: "刚刚",
        available: true,
      }],
    });
    await openMenu(context);
    fireEvent.pointerDown(screen.getByText("report.pdf"), { button: 0 });
    await waitFor(() => expect(dragAdapter.startFiles).toHaveBeenCalledWith(
      expect.objectContaining({ slotId: "slot-files", revision: 4 }),
      expect.objectContaining({ leaseId: "lease-1" }),
      expect.any(Function),
    ));
  });

  it("uses paused state for quick actions", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, { ...liveState, publishPaused: true, subscribePaused: true });
    await openMenu(context);
    fireEvent.click(screen.getByRole("button", { name: "恢复" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, expect.objectContaining({ action: "toggle-sync", requestId: expect.any(String) }));
    fireEvent.click(screen.getByRole("button", { name: "隐藏" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, expect.objectContaining({ action: "hide-orb", requestId: expect.any(String) }));
  });

  it("keeps the menu mounted while the closing animation finishes", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    await openMenu(context);

    fireEvent.click(screen.getByRole("button", { name: "关闭快捷菜单" }));
    await waitFor(() => expect(screen.getByRole("region", { name: "AirDrop 快捷菜单" }).closest("main")).toHaveClass("is-closing"));
    expect(context.adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false }));

    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false })), { timeout: 1_000 });
    const layout = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
      event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded,
    ).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: true });
    expect(await screen.findByRole("button", { name: /AirDrop 悬浮球/ })).toBeInTheDocument();
  });

  it("removes all native listeners on unmount", async () => {
    const context = setup();
    const view = render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    view.unmount();
    expect(context.unlisteners).toHaveLength(4);
    context.unlisteners.forEach((unlisten) => expect(unlisten).toHaveBeenCalledOnce());
  });
});
