import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
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

const expand = async (context: ReturnType<typeof setup>) => {
  fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
  const layout = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
    event === FLOATING_EVENTS.layout && (payload as { expanded: boolean }).expanded,
  ).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
  context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: true });
  await screen.findByRole("region", { name: "AirDrop 悬浮操作面板" });
};

describe("FloatingOrbApp", () => {
  it("renders only the floating surface for the floating route", () => {
    const createClient = vi.fn();
    render(<AirDropSurface search="?surface=floating" createClient={createClient} />);
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "主导航" })).not.toBeInTheDocument();
    expect(createClient).not.toHaveBeenCalled();
  });

  it("installs state and layout listeners before announcing ready", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, { protocolVersion: 1 }));
    expect(context.calls).toEqual([
      `listen:${FLOATING_EVENTS.state}`,
      `listen:${FLOATING_EVENTS.layoutState}`,
      `emit:${FLOATING_EVENTS.ready}`,
    ]);
  });

  it("renders expanded controls only after the matching successful layout ack", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
    const layout = vi.mocked(context.adapter.emit).mock.calls.find(([event]) => event === FLOATING_EVENTS.layout)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    expect(screen.queryByRole("region", { name: "AirDrop 悬浮操作面板" })).not.toBeInTheDocument();
    context.dispatch(FLOATING_EVENTS.layoutState, { requestId: "other", expanded: true, success: true });
    expect(screen.queryByRole("region", { name: "AirDrop 悬浮操作面板" })).not.toBeInTheDocument();
    context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: true });
    expect(await screen.findByRole("region", { name: "AirDrop 悬浮操作面板" })).toBeInTheDocument();
  });

  it("stays collapsed and exposes a useful status when layout fails", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
    const layout = vi.mocked(context.adapter.emit).mock.calls.find(([event]) => event === FLOATING_EVENTS.layout)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    context.dispatch(FLOATING_EVENTS.layoutState, { ...layout, success: false, message: "窗口空间不足" });
    await waitFor(() => expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeEnabled());
    expect(screen.getByRole("status")).toHaveTextContent("窗口空间不足");
  });

  it("uses paused state for the sync label and emits exact action values", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    context.dispatch(FLOATING_EVENTS.state, { ...liveState, publishPaused: true, subscribePaused: true });
    await expand(context);
    fireEvent.click(screen.getByRole("button", { name: "恢复同步" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "toggle-sync" });
    fireEvent.click(screen.getByRole("button", { name: "禁用悬浮球" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "hide-orb" });
  });

  it("starts native dragging and consumes a long drag's trailing click without blocking a later intentional click", async () => {
    const context = setup();
    const now = vi.spyOn(Date, "now").mockReturnValue(1);
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    const handle = screen.getByRole("button", { name: "拖动悬浮球" });
    const styles = readFileSync(resolve(process.cwd(), "src/features/floating/floating.css"), "utf8");
    expect(styles).toMatch(/\.floating-drag-handle\s*\{[^}]*width:\s*36px;[^}]*height:\s*36px;/s);
    expect(styles).toMatch(/\.floating-drag-handle-collapsed\s*\{[^}]*top:\s*0;/s);
    fireEvent.pointerDown(handle);
    expect(context.adapter.startDragging).toHaveBeenCalledOnce();
    now.mockReturnValue(60_001);
    fireEvent.click(handle);
    expect(context.adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.anything());
    fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: true }));
    now.mockRestore();
  });

  it("recovers a timed-out expand with a matching collapse transaction and ignores the late expand ack", async () => {
    vi.useFakeTimers();
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await vi.waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
    const expandRequest = vi.mocked(context.adapter.emit).mock.calls.find(([event, payload]) =>
      event === FLOATING_EVENTS.layout && (payload as { expanded: boolean }).expanded,
    )?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeDisabled();
    await act(() => vi.advanceTimersByTimeAsync(2_000));
    const recoveryRequest = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
      event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded,
    ).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    expect(recoveryRequest.requestId).not.toBe(expandRequest.requestId);
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeDisabled();
    context.dispatch(FLOATING_EVENTS.layoutState, { ...expandRequest, success: true });
    expect(screen.queryByRole("region", { name: "AirDrop 悬浮操作面板" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeDisabled();
    act(() => context.dispatch(FLOATING_EVENTS.layoutState, { ...recoveryRequest, success: true }));
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("悬浮球已收起");
    vi.useRealTimers();
  });

  it("bounds recovery attempts and unlocks with an accessible error", async () => {
    vi.useFakeTimers();
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await vi.waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    fireEvent.click(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" }));
    await act(() => vi.advanceTimersByTimeAsync(4_000));
    expect(vi.mocked(context.adapter.emit).mock.calls.filter(([event]) => event === FLOATING_EVENTS.layout)).toHaveLength(2);
    expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("状态可能异常");
    vi.useRealTimers();
  });

  it("moves focus into expanded actions and restores it to the droplet after collapse ack", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    await expand(context);
    await waitFor(() => expect(screen.getByRole("button", { name: "打开剪贴板" })).toHaveFocus());
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false })));
    const collapse = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
      event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded,
    ).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    context.dispatch(FLOATING_EVENTS.layoutState, { ...collapse, success: true });
    await waitFor(() => expect(screen.getByRole("button", { name: "展开 AirDrop 悬浮球" })).toHaveFocus());
  });

  it("does not update or schedule collapse when an action completes after unmount", async () => {
    const context = setup();
    let resolveAction!: () => void;
    const actionCompletion = new Promise<void>((resolve) => { resolveAction = resolve; });
    vi.mocked(context.adapter.emit).mockImplementation(((event: keyof FloatingEventPayloads) =>
      event === FLOATING_EVENTS.action ? actionCompletion : Promise.resolve()) as FloatingOrbWindowAdapter["emit"]);
    const view = render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    await expand(context);
    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "打开主窗口" }));
    view.unmount();
    resolveAction();
    await act(async () => { await actionCompletion; await vi.advanceTimersByTimeAsync(500); });
    expect(vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) =>
      event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded,
    )).toHaveLength(0);
    vi.useRealTimers();
  });

  it("schedules a collapse request after a successful expanded action", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    await expand(context);
    fireEvent.click(screen.getByRole("button", { name: "打开主窗口" }));
    expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.action, { action: "open-main" });
    expect(context.adapter.emit).not.toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false }));
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false })));
  });

  it("requests collapse on Escape and focus loss and waits for ack", async () => {
    const context = setup();
    render(<FloatingOrbApp adapter={context.adapter} />);
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    await expand(context);
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.layout, expect.objectContaining({ expanded: false })));
    expect(screen.getByRole("region", { name: "AirDrop 悬浮操作面板" })).toBeInTheDocument();
    const collapse = vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) => event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded).at(-1)?.[1] as FloatingEventPayloads[typeof FLOATING_EVENTS.layout];
    context.dispatch(FLOATING_EVENTS.layoutState, { ...collapse, success: true });
    await screen.findByRole("button", { name: "展开 AirDrop 悬浮球" });

    await expand(context);
    fireEvent.blur(window);
    await waitFor(() => expect(vi.mocked(context.adapter.emit).mock.calls.filter(([event, payload]) => event === FLOATING_EVENTS.layout && !(payload as { expanded: boolean }).expanded)).toHaveLength(2));
  });

  it("removes native and browser listeners and clears timers on unmount", async () => {
    vi.useFakeTimers();
    const context = setup();
    const remove = vi.spyOn(window, "removeEventListener");
    const view = render(<FloatingOrbApp adapter={context.adapter} />);
    await vi.waitFor(() => expect(context.adapter.emit).toHaveBeenCalledWith(FLOATING_EVENTS.ready, expect.anything()));
    view.unmount();
    expect(context.unlisteners).toHaveLength(2);
    context.unlisteners.forEach((unlisten) => expect(unlisten).toHaveBeenCalledOnce());
    expect(remove).toHaveBeenCalledWith("blur", expect.any(Function));
    expect(remove).toHaveBeenCalledWith("keydown", expect.any(Function));
    remove.mockRestore();
    vi.useRealTimers();
  });
});
