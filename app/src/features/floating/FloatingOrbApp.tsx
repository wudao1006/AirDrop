import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Icon, type IconName } from "../../components/Icon";
import { applyAppearanceSettings, loadAppearanceSettings } from "../settings/appearance";
import {
  FLOATING_EVENTS,
  type FloatingEventPayloads,
  type FloatingOrbActionPayload,
  type FloatingOrbLayoutStatePayload,
  type FloatingOrbStatePayload,
  type FloatingSlotSummary,
} from "./floating-events";
import "./floating.css";

export interface FloatingOrbWindowAdapter {
  emit<K extends keyof FloatingEventPayloads>(event: K, payload: FloatingEventPayloads[K]): Promise<void>;
  listen<K extends keyof FloatingEventPayloads>(event: K, handler: (payload: FloatingEventPayloads[K]) => void): Promise<UnlistenFn>;
  startDragging(): Promise<void>;
}

const defaultAdapter: FloatingOrbWindowAdapter = {
  emit: (event, payload) => emit(event, payload),
  listen: async (event, handler) => listen(event, ({ payload }) => handler(payload as never)),
  startDragging: async () => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) return;
    await getCurrentWindow().startDragging();
  },
};

const requestId = (): string => globalThis.crypto?.randomUUID?.()
  ?? `orb-${Date.now()}-${Math.random().toString(36).slice(2)}`;

const FlowMark = () => <svg className="floating-flow-mark" viewBox="0 0 40 34" aria-hidden="true">
  <path className="flow-stroke flow-stroke-a" d="M7 12c6-7 13-7 18-2 3 3 5 4 8 3" />
  <path className="flow-stroke flow-stroke-b" d="M7 23c5 5 12 5 17 0 3-3 5-4 9-3" />
  <circle cx="7" cy="12" r="2.2" /><circle cx="33" cy="20" r="2.2" />
</svg>;

const platformIcon: Record<FloatingSlotSummary["platform"], IconName> = {
  macos: "apple",
  windows: "windows",
  linux: "linux",
  android: "phone",
};

const kindIcon: Record<FloatingSlotSummary["kind"], IconName> = {
  text: "text",
  html: "code",
  image: "image",
  url: "link",
  files: "files",
  private: "shield",
};

interface FloatingOrbAppProps {
  adapter?: FloatingOrbWindowAdapter;
}

const LAYOUT_ACK_TIMEOUT_MS = 2_000;
const MENU_WIDTH = 356;
const menuHeight = (slotCount: number): number => Math.min(520, 132 + Math.max(1, slotCount) * 76);

export function FloatingOrbApp({ adapter = defaultAdapter }: FloatingOrbAppProps) {
  const [state, setState] = useState<FloatingOrbStatePayload | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [pendingLayout, setPendingLayout] = useState<{ requestId: string; expanded: boolean } | null>(null);
  const [status, setStatus] = useState("正在连接主窗口…");
  const [usingSlotId, setUsingSlotId] = useState<string | null>(null);
  const pendingRef = useRef(pendingLayout);
  const expandedRef = useRef(expanded);
  const stateRef = useRef(state);
  const mountedRef = useRef(true);
  const collapseTimerRef = useRef<number | undefined>(undefined);
  const layoutAckTimerRef = useRef<number | undefined>(undefined);
  const firstActionRef = useRef<HTMLButtonElement>(null);
  pendingRef.current = pendingLayout;
  expandedRef.current = expanded;
  stateRef.current = state;

  const requestLayout = useCallback(function issueLayout(nextExpanded: boolean, recovery = false) {
    if (!mountedRef.current || pendingRef.current || (!recovery && nextExpanded === expandedRef.current)) return;
    const request = {
      requestId: requestId(),
      expanded: nextExpanded,
      width: nextExpanded ? MENU_WIDTH : undefined,
      height: nextExpanded ? menuHeight(stateRef.current?.slots.length ?? 0) : undefined,
    };
    pendingRef.current = request;
    setPendingLayout(request);
    setStatus(nextExpanded ? "正在打开快捷菜单…" : "正在收起悬浮球…");
    window.clearTimeout(layoutAckTimerRef.current);
    layoutAckTimerRef.current = window.setTimeout(() => {
      if (!mountedRef.current || pendingRef.current?.requestId !== request.requestId) return;
      pendingRef.current = null;
      setPendingLayout(null);
      if (nextExpanded && !recovery) issueLayout(false, true);
      else setStatus("悬浮窗布局调整超时");
    }, LAYOUT_ACK_TIMEOUT_MS);
    void adapter.emit(FLOATING_EVENTS.layout, request).catch(() => {
      if (!mountedRef.current || pendingRef.current?.requestId !== request.requestId) return;
      window.clearTimeout(layoutAckTimerRef.current);
      pendingRef.current = null;
      setPendingLayout(null);
      setStatus("无法调整悬浮球布局");
    });
  }, [adapter]);

  const scheduleCollapse = useCallback(() => {
    window.clearTimeout(collapseTimerRef.current);
    collapseTimerRef.current = window.setTimeout(() => requestLayout(false), 100);
  }, [requestLayout]);

  const runAction = useCallback((payload: FloatingOrbActionPayload) => {
    if (payload.action === "use-slot") setUsingSlotId(payload.slotId);
    void adapter.emit(FLOATING_EVENTS.action, payload).then(() => {
      if (!mountedRef.current) return;
      setUsingSlotId(null);
      setStatus(payload.action === "use-slot" ? "已取入本机剪贴板" : "操作已发送");
      scheduleCollapse();
    }).catch(() => {
      if (!mountedRef.current) return;
      setUsingSlotId(null);
      setStatus("操作失败，请打开主窗口查看");
    });
  }, [adapter, scheduleCollapse]);

  useEffect(() => {
    let disposed = false;
    mountedRef.current = true;
    const unlisteners: UnlistenFn[] = [];

    const install = async () => {
      const stateUnlisten = await adapter.listen(FLOATING_EVENTS.state, (nextState) => {
        if (disposed) return;
        setState(nextState);
        setStatus(nextState.busy ? "正在同步" : nextState.activity === "reconnecting" ? "正在重新连接" : "同步就绪");
        applyAppearanceSettings({ ...loadAppearanceSettings(), ...nextState.appearance });
      });
      if (disposed) { stateUnlisten(); return; }
      unlisteners.push(stateUnlisten);

      const layoutUnlisten = await adapter.listen(FLOATING_EVENTS.layoutState, (layout: FloatingOrbLayoutStatePayload) => {
        if (disposed || layout.requestId !== pendingRef.current?.requestId) return;
        window.clearTimeout(layoutAckTimerRef.current);
        pendingRef.current = null;
        setPendingLayout(null);
        if (layout.success) {
          setExpanded(layout.expanded);
          setStatus(layout.expanded ? "快捷菜单已打开" : "悬浮球已收起");
        } else {
          setStatus(layout.message ?? "无法调整悬浮球布局");
        }
      });
      if (disposed) { layoutUnlisten(); return; }
      unlisteners.push(layoutUnlisten);

      const openMenuUnlisten = await adapter.listen(FLOATING_EVENTS.openMenu, () => requestLayout(true));
      if (disposed) { openMenuUnlisten(); return; }
      unlisteners.push(openMenuUnlisten);
      await adapter.emit(FLOATING_EVENTS.ready, { protocolVersion: 1 });
    };

    void install().catch(() => { if (!disposed) setStatus("无法连接主窗口"); });
    const collapse = () => { if (expandedRef.current) requestLayout(false); };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") collapse(); };
    window.addEventListener("blur", collapse);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      disposed = true;
      mountedRef.current = false;
      window.clearTimeout(collapseTimerRef.current);
      window.clearTimeout(layoutAckTimerRef.current);
      window.removeEventListener("blur", collapse);
      window.removeEventListener("keydown", onKeyDown);
      unlisteners.splice(0).forEach((unlisten) => unlisten());
    };
  }, [adapter, requestLayout]);

  useEffect(() => {
    if (expanded) firstActionRef.current?.focus();
  }, [expanded]);

  const startDragging = (event: React.PointerEvent<HTMLElement>) => {
    if (event.button > 0) return;
    event.preventDefault();
    void adapter.startDragging().catch(() => { if (mountedRef.current) setStatus("当前环境不支持拖动"); });
  };
  const openContextMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    requestLayout(true);
  };
  const paused = Boolean(state?.publishPaused && state?.subscribePaused);
  const inactive = paused || state?.activity === "suspended";

  return <main className={`floating-orb-root ${expanded ? "is-expanded" : "is-collapsed"} ${inactive ? "is-paused" : "is-live"}`}>
    {expanded ? <section className="floating-menu" aria-label="AirDrop 快捷菜单" onContextMenu={(event) => event.preventDefault()}>
      <header className="floating-menu-header" onPointerDown={startDragging}>
        <span className="floating-menu-mark"><FlowMark /></span>
        <span><strong>设备剪贴板</strong><small>{state?.slots.length ? "选择内容即可取入本机" : "等待其他设备内容"}</small></span>
        <button ref={firstActionRef} type="button" aria-label="关闭快捷菜单" onPointerDown={(event) => event.stopPropagation()} onClick={() => requestLayout(false)}><Icon name="x" size={15} /></button>
      </header>
      <div className="floating-device-list">
        {state?.slots.length ? state.slots.map((slot) => <article className="floating-device-item" key={`${slot.id}-${slot.revision}`}>
          <span className="floating-device-icon"><Icon name={platformIcon[slot.platform]} size={17} /><i><Icon name={kindIcon[slot.kind]} size={10} /></i></span>
          <span className="floating-device-copy">
            <span className="floating-device-title"><strong>{slot.deviceName}</strong><small>{slot.ageLabel}</small></span>
            {slot.imagePreview ? <img src={slot.imagePreview} alt={slot.preview} />
              : slot.fileNames?.length ? <span className="floating-file-names">{slot.fileNames.join(" · ")}</span>
                : <span className="floating-device-preview">{slot.preview}</span>}
          </span>
          <button type="button" disabled={!slot.available || usingSlotId !== null} onClick={() => runAction({ action: "use-slot", slotId: slot.id, revision: slot.revision })}>{usingSlotId === slot.id ? "取入中" : "使用"}</button>
        </article>) : <div className="floating-empty"><Icon name="devices" size={22} /><span>暂无可用的设备剪贴板</span></div>}
      </div>
      <footer className="floating-menu-actions">
        <button type="button" disabled={!state?.canReadClipboard || state.busy} onClick={() => runAction({ action: "publish-current" })}><Icon name="refresh" /><span>刷新</span></button>
        <button type="button" onClick={() => runAction({ action: "toggle-sync" })}><Icon name={paused ? "play" : "pause"} /><span>{paused ? "恢复" : "暂停"}</span></button>
        <button type="button" onClick={() => runAction({ action: "open-clipboard" })}><Icon name="clipboard" /><span>剪贴板</span></button>
        <button type="button" onClick={() => runAction({ action: "open-main" })}><Icon name="monitor" /><span>主窗口</span></button>
        <button type="button" onClick={() => runAction({ action: "hide-orb" })}><Icon name="x" /><span>隐藏</span></button>
      </footer>
    </section> : <button
      className="floating-droplet"
      type="button"
      aria-label="AirDrop 悬浮球，左键拖动，右键打开菜单"
      title="左键拖动 · 右键打开菜单"
      disabled={Boolean(pendingLayout)}
      onPointerDown={startDragging}
      onContextMenu={openContextMenu}
    >
      <span className="floating-lobe floating-lobe-cyan" /><span className="floating-lobe floating-lobe-blue" />
      <span className="floating-liquid-edge" /><FlowMark />
    </button>}
    <span className="sr-only" role="status" aria-live="polite">{status}</span>
  </main>;
}
