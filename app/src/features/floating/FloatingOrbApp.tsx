import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Icon } from "../../components/Icon";
import { applyAppearanceSettings, loadAppearanceSettings } from "../settings/appearance";
import {
  FLOATING_EVENTS,
  type FloatingEventPayloads,
  type FloatingOrbAction,
  type FloatingOrbLayoutStatePayload,
  type FloatingOrbStatePayload,
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

interface FloatingOrbAppProps {
  adapter?: FloatingOrbWindowAdapter;
}

const LAYOUT_ACK_TIMEOUT_MS = 2_000;

export function FloatingOrbApp({ adapter = defaultAdapter }: FloatingOrbAppProps) {
  const [state, setState] = useState<FloatingOrbStatePayload | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [pendingLayout, setPendingLayout] = useState<{ requestId: string; expanded: boolean } | null>(null);
  const [status, setStatus] = useState("正在连接主窗口…");
  const pendingRef = useRef(pendingLayout);
  const expandedRef = useRef(expanded);
  const mountedRef = useRef(true);
  const collapseTimerRef = useRef<number | undefined>(undefined);
  const layoutAckTimerRef = useRef<number | undefined>(undefined);
  const focusTargetRef = useRef<"expanded" | "collapsed" | null>(null);
  const dropletRef = useRef<HTMLButtonElement>(null);
  const firstActionRef = useRef<HTMLButtonElement>(null);
  pendingRef.current = pendingLayout;
  expandedRef.current = expanded;

  const requestLayout = useCallback(function issueLayout(nextExpanded: boolean, recovery = false) {
    if (!mountedRef.current || pendingRef.current || (!recovery && nextExpanded === expandedRef.current)) return;
    const request = { requestId: requestId(), expanded: nextExpanded };
    pendingRef.current = request;
    setPendingLayout(request);
    setStatus(nextExpanded ? "正在展开悬浮球…" : "正在收起悬浮球…");
    window.clearTimeout(layoutAckTimerRef.current);
    layoutAckTimerRef.current = window.setTimeout(() => {
      if (!mountedRef.current || pendingRef.current?.requestId !== request.requestId) return;
      pendingRef.current = null;
      setPendingLayout(null);
      if (nextExpanded && !recovery) {
        setStatus("展开确认超时，正在恢复折叠布局…");
        issueLayout(false, true);
      } else if (recovery) {
        setStatus("主窗口未确认恢复折叠，悬浮窗状态可能异常");
      } else {
        setStatus("主窗口未确认收起，操作面板保持展开");
      }
    }, LAYOUT_ACK_TIMEOUT_MS);
    void adapter.emit(FLOATING_EVENTS.layout, request).catch(() => {
      if (!mountedRef.current || pendingRef.current?.requestId !== request.requestId) return;
      window.clearTimeout(layoutAckTimerRef.current);
      pendingRef.current = null;
      setPendingLayout(null);
      setStatus(recovery ? "无法恢复折叠布局，悬浮窗状态可能异常" : "无法调整悬浮球布局");
    });
  }, [adapter]);

  const scheduleCollapse = useCallback(() => {
    window.clearTimeout(collapseTimerRef.current);
    collapseTimerRef.current = window.setTimeout(() => {
      if (mountedRef.current) requestLayout(false);
    }, 80);
  }, [requestLayout]);

  const runAction = useCallback((action: FloatingOrbAction) => {
    void adapter.emit(FLOATING_EVENTS.action, { action }).then(() => {
      if (!mountedRef.current) return;
      setStatus("操作已发送");
      if (expandedRef.current) scheduleCollapse();
    }).catch(() => { if (mountedRef.current) setStatus("操作发送失败"); });
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
          focusTargetRef.current = layout.expanded ? "expanded" : "collapsed";
          setExpanded(layout.expanded);
          setStatus(layout.expanded ? "操作面板已展开" : "悬浮球已收起");
        } else {
          setStatus(layout.message ?? "无法调整悬浮球布局");
        }
      });
      if (disposed) { layoutUnlisten(); return; }
      unlisteners.push(layoutUnlisten);

      await adapter.emit(FLOATING_EVENTS.ready, { protocolVersion: 1 });
    };

    void install().catch(() => { if (!disposed) setStatus("无法连接主窗口"); });
    const collapse = () => { if (expandedRef.current) requestLayout(false); };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") collapse();
    };
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
    if (focusTargetRef.current === "expanded" && expanded) {
      firstActionRef.current?.focus();
      focusTargetRef.current = null;
    } else if (focusTargetRef.current === "collapsed" && !expanded) {
      dropletRef.current?.focus();
      focusTargetRef.current = null;
    }
  }, [expanded]);

  const startDragging = () => {
    void adapter.startDragging().catch(() => { if (mountedRef.current) setStatus("当前环境不支持拖动"); });
  };
  const finishDragging = (event: React.MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
  };
  const paused = Boolean(state?.publishPaused && state?.subscribePaused);
  const inactive = paused || state?.activity === "suspended";

  return <main className={`floating-orb-root ${expanded ? "is-expanded" : "is-collapsed"} ${inactive ? "is-paused" : "is-live"}`}>
    {expanded ? <section className="floating-capsule" aria-label="AirDrop 悬浮操作面板">
      <button className="floating-drag-handle" type="button" aria-label="拖动悬浮窗" title="拖动悬浮窗" onPointerDown={startDragging} onClick={finishDragging}><span /></button>
      <div className="floating-capsule-mark"><FlowMark /></div>
      <div className="floating-actions">
        <button ref={firstActionRef} type="button" aria-label="打开剪贴板" title="打开剪贴板" onClick={() => runAction("open-clipboard")}><Icon name="clipboard" /></button>
        <button type="button" aria-label="刷新本机剪贴板" title="刷新本机剪贴板" disabled={!state?.canReadClipboard || state.busy} onClick={() => runAction("publish-current")}><Icon name="transfer" /></button>
        <button type="button" aria-label={paused ? "恢复同步" : "暂停同步"} title={paused ? "恢复同步" : "暂停同步"} onClick={() => runAction("toggle-sync")}><Icon name={paused ? "play" : "pause"} /></button>
        <button type="button" aria-label="打开主窗口" title="打开主窗口" onClick={() => runAction("open-main")}><Icon name="monitor" /></button>
        <button type="button" className="floating-hide-action" aria-label="禁用悬浮球" title="禁用悬浮球" onClick={() => runAction("hide-orb")}><Icon name="x" /></button>
      </div>
    </section> : <div className="floating-collapsed-shell">
      <button className="floating-drag-handle floating-drag-handle-collapsed" type="button" aria-label="拖动悬浮球" title="拖动悬浮球" onPointerDown={startDragging} onClick={finishDragging}><span /></button>
      <button ref={dropletRef} className="floating-droplet" type="button" aria-label="展开 AirDrop 悬浮球" aria-expanded="false" disabled={Boolean(pendingLayout)} onClick={() => requestLayout(true)}>
        <span className="floating-lobe floating-lobe-cyan" /><span className="floating-lobe floating-lobe-blue" />
        <span className="floating-liquid-edge" /><FlowMark />
      </button>
    </div>}
    <span className="sr-only" role="status" aria-live="polite">{status}</span>
  </main>;
}
