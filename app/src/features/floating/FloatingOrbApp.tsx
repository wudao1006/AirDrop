import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Icon, type IconName } from "../../components/Icon";
import { applyAppearanceSettings, loadAppearanceSettings } from "../settings/appearance";
import {
  FLOATING_EVENTS,
  type FloatingEventPayloads,
  type FloatingOrbActionCommand,
  type FloatingOrbActionResultPayload,
  type FloatingOrbLayoutStatePayload,
  type FloatingOrbStatePayload,
  type FloatingSlotSummary,
} from "./floating-events";
import {
  createFloatingContentDragAdapter,
  type FloatingContentDragAdapter,
  type PreparedSlotDrag,
} from "./floating-drag";
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

const kindLabel: Record<FloatingSlotSummary["kind"], string> = {
  text: "文本",
  html: "富文本",
  image: "图片",
  url: "链接",
  files: "文件",
  private: "私有格式",
};

const usesDataTransfer = (kind: FloatingSlotSummary["kind"]): boolean =>
  kind === "text" || kind === "url" || kind === "html";

const dragKey = (slot: Pick<FloatingSlotSummary, "id" | "revision">): string =>
  `${slot.id}:${slot.revision}`;

interface FloatingOrbAppProps {
  adapter?: FloatingOrbWindowAdapter;
  dragAdapter?: FloatingContentDragAdapter;
}

const LAYOUT_ACK_TIMEOUT_MS = 2_000;
const MENU_TRANSITION_MS = 240;
const MENU_WIDTH = 356;
const menuHeight = (slotCount: number): number => Math.min(520, 158 + Math.max(1, slotCount) * 76);
type FloatingSurfacePhase = "collapsed" | "opening" | "expanded" | "closing";

const transitionDuration = (): number => typeof window !== "undefined"
  && typeof window.matchMedia === "function"
  && window.matchMedia("(prefers-reduced-motion: reduce)").matches
  ? 0
  : MENU_TRANSITION_MS;

export function FloatingOrbApp({
  adapter = defaultAdapter,
  dragAdapter = createFloatingContentDragAdapter(),
}: FloatingOrbAppProps) {
  const [state, setState] = useState<FloatingOrbStatePayload | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [surfacePhase, setSurfacePhase] = useState<FloatingSurfacePhase>("collapsed");
  const [anchor, setAnchor] = useState({ x: 0.91, y: 0.08 });
  const [pendingLayout, setPendingLayout] = useState<{ requestId: string; expanded: boolean } | null>(null);
  const [status, setStatus] = useState("正在连接主窗口…");
  const [usingSlotId, setUsingSlotId] = useState<string | null>(null);
  const [draggingSlotId, setDraggingSlotId] = useState<string | null>(null);
  const [preparedDrags, setPreparedDrags] = useState<Record<string, PreparedSlotDrag>>({});
  const pendingRef = useRef(pendingLayout);
  const expandedRef = useRef(expanded);
  const surfacePhaseRef = useRef(surfacePhase);
  const stateRef = useRef(state);
  const preparedDragsRef = useRef(preparedDrags);
  const pendingActionsRef = useRef(new Map<string, FloatingOrbActionCommand>());
  const mountedRef = useRef(true);
  const collapseTimerRef = useRef<number | undefined>(undefined);
  const transitionTimerRef = useRef<number | undefined>(undefined);
  const layoutAckTimerRef = useRef<number | undefined>(undefined);
  const firstActionRef = useRef<HTMLButtonElement>(null);
  pendingRef.current = pendingLayout;
  expandedRef.current = expanded;
  surfacePhaseRef.current = surfacePhase;
  stateRef.current = state;
  preparedDragsRef.current = preparedDrags;

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
      else {
        if (!nextExpanded && expandedRef.current) {
          surfacePhaseRef.current = "expanded";
          setSurfacePhase("expanded");
        }
        setStatus("悬浮窗布局调整超时");
      }
    }, LAYOUT_ACK_TIMEOUT_MS);
    void adapter.emit(FLOATING_EVENTS.layout, request).catch(() => {
      if (!mountedRef.current || pendingRef.current?.requestId !== request.requestId) return;
      window.clearTimeout(layoutAckTimerRef.current);
      pendingRef.current = null;
      setPendingLayout(null);
      if (!nextExpanded && expandedRef.current) {
        surfacePhaseRef.current = "expanded";
        setSurfacePhase("expanded");
      }
      setStatus("无法调整悬浮球布局");
    });
  }, [adapter]);

  const beginCollapse = useCallback(() => {
    if (!mountedRef.current || !expandedRef.current || surfacePhaseRef.current === "closing") return;
    window.clearTimeout(transitionTimerRef.current);
    surfacePhaseRef.current = "closing";
    setSurfacePhase("closing");
    setStatus("正在收起悬浮球…");
    transitionTimerRef.current = window.setTimeout(() => requestLayout(false), transitionDuration());
  }, [requestLayout]);

  const scheduleCollapse = useCallback(() => {
    window.clearTimeout(collapseTimerRef.current);
    collapseTimerRef.current = window.setTimeout(beginCollapse, 120);
  }, [beginCollapse]);

  const runAction = useCallback((command: FloatingOrbActionCommand) => {
    const actionRequestId = requestId();
    pendingActionsRef.current.set(actionRequestId, command);
    if (command.action === "use-slot") setUsingSlotId(command.slotId);
    setStatus(command.action === "use-slot" ? "正在写入本机剪贴板…" : "正在执行操作…");
    void adapter.emit(FLOATING_EVENTS.action, { ...command, requestId: actionRequestId }).catch(() => {
      if (!mountedRef.current) return;
      pendingActionsRef.current.delete(actionRequestId);
      setUsingSlotId(null);
      setStatus("操作失败，请打开主窗口查看");
    });
  }, [adapter]);

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
          expandedRef.current = layout.expanded;
          setExpanded(layout.expanded);
          window.clearTimeout(transitionTimerRef.current);
          if (layout.expanded) {
            if (layout.anchor) setAnchor(layout.anchor);
            surfacePhaseRef.current = "opening";
            setSurfacePhase("opening");
            setStatus("快捷菜单已打开");
            transitionTimerRef.current = window.setTimeout(() => {
              if (!mountedRef.current || surfacePhaseRef.current !== "opening") return;
              surfacePhaseRef.current = "expanded";
              setSurfacePhase("expanded");
            }, transitionDuration());
          } else {
            surfacePhaseRef.current = "collapsed";
            setSurfacePhase("collapsed");
            setStatus("悬浮球已收起");
          }
        } else {
          surfacePhaseRef.current = expandedRef.current ? "expanded" : "collapsed";
          setSurfacePhase(surfacePhaseRef.current);
          setStatus(layout.message ?? "无法调整悬浮球布局");
        }
      });
      if (disposed) { layoutUnlisten(); return; }
      unlisteners.push(layoutUnlisten);

      const openMenuUnlisten = await adapter.listen(FLOATING_EVENTS.openMenu, () => requestLayout(true));
      if (disposed) { openMenuUnlisten(); return; }
      unlisteners.push(openMenuUnlisten);

      const actionResultUnlisten = await adapter.listen(FLOATING_EVENTS.actionResult, (result: FloatingOrbActionResultPayload) => {
        if (disposed) return;
        const command = pendingActionsRef.current.get(result.requestId);
        if (!command) return;
        pendingActionsRef.current.delete(result.requestId);
        if (command.action === "use-slot") setUsingSlotId(null);
        setStatus(result.message);
        if (result.success) scheduleCollapse();
      });
      if (disposed) { actionResultUnlisten(); return; }
      unlisteners.push(actionResultUnlisten);
      await adapter.emit(FLOATING_EVENTS.ready, { protocolVersion: 1 });
    };

    void install().catch(() => { if (!disposed) setStatus("无法连接主窗口"); });
    const collapse = () => { if (expandedRef.current) beginCollapse(); };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") collapse(); };
    window.addEventListener("blur", collapse);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      disposed = true;
      mountedRef.current = false;
      window.clearTimeout(collapseTimerRef.current);
      window.clearTimeout(transitionTimerRef.current);
      window.clearTimeout(layoutAckTimerRef.current);
      window.removeEventListener("blur", collapse);
      window.removeEventListener("keydown", onKeyDown);
      pendingActionsRef.current.clear();
      unlisteners.splice(0).forEach((unlisten) => unlisten());
    };
  }, [adapter, beginCollapse, requestLayout, scheduleCollapse]);

  useEffect(() => {
    if (expanded) firstActionRef.current?.focus();
  }, [expanded]);

  useEffect(() => {
    if (!expanded || !dragAdapter.supported || !state?.slots.length) {
      setPreparedDrags({});
      return;
    }
    let disposed = false;
    const textualSlots = state.slots.filter((slot) => slot.available && usesDataTransfer(slot.kind));
    const validKeys = new Set(textualSlots.map(dragKey));
    setPreparedDrags((current) => {
      const entries = Object.entries(current).filter(([key]) => validKeys.has(key));
      return entries.length === Object.keys(current).length ? current : Object.fromEntries(entries);
    });
    for (const slot of textualSlots) {
      const key = dragKey(slot);
      if (preparedDragsRef.current[key]) continue;
      void dragAdapter.prepare(slot.id, slot.revision).then((prepared) => {
        if (!disposed) setPreparedDrags((current) => ({ ...current, [key]: prepared }));
      }).catch(() => {
        if (!disposed) setStatus("部分内容暂时无法拖出，可使用“使用”按钮");
      });
    }
    return () => { disposed = true; };
  }, [dragAdapter, expanded, state?.slots]);

  const startDragging = (event: React.PointerEvent<HTMLElement>) => {
    if (event.button > 0) return;
    event.preventDefault();
    void adapter.startDragging().catch(() => { if (mountedRef.current) setStatus("当前环境不支持拖动"); });
  };
  const startFileDrag = (event: React.PointerEvent<HTMLElement>, slot: FloatingSlotSummary) => {
    if (event.button > 0 || !slot.available || draggingSlotId !== null) return;
    if (usesDataTransfer(slot.kind)) {
      if (!preparedDrags[dragKey(slot)]) setStatus("正在准备完整内容，请稍后再拖");
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (!dragAdapter.supported) {
      setStatus("当前环境不支持跨应用拖放，请使用“使用”按钮");
      return;
    }
    setDraggingSlotId(slot.id);
    setStatus("拖到目标应用后松开即可直接插入");
    void dragAdapter.prepare(slot.id, slot.revision).then((prepared) => dragAdapter.startFiles({
      slotId: slot.id,
      revision: slot.revision,
      deviceName: slot.deviceName,
      kind: slot.kind,
      preview: slot.preview,
    }, prepared, (result) => {
      if (!mountedRef.current) return;
      setDraggingSlotId(null);
      setStatus(result.result.toLowerCase().includes("cancel") ? "已取消拖放" : "内容已送入目标应用");
    })).catch(() => {
      if (!mountedRef.current) return;
      setDraggingSlotId(null);
      setStatus("无法拖出此内容，请使用“使用”按钮");
    });
  };
  const startDataDrag = (event: React.DragEvent<HTMLElement>, slot: FloatingSlotSummary) => {
    const prepared = preparedDrags[dragKey(slot)];
    if (!prepared || Array.isArray(prepared.item)) {
      event.preventDefault();
      setStatus("内容仍在准备，请稍后再拖");
      return;
    }
    const { data, types } = prepared.item;
    for (const type of types) {
      const value = typeof data === "string" ? data : data[type];
      if (value !== undefined) event.dataTransfer.setData(type, value);
    }
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setDragImage(event.currentTarget, 18, 18);
    setDraggingSlotId(slot.id);
    setStatus("拖到目标应用后松开即可直接插入");
  };
  const finishDataDrag = () => {
    setDraggingSlotId(null);
    setStatus("拖放已结束");
  };
  const openContextMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    requestLayout(true);
  };
  const paused = Boolean(state?.publishPaused && state?.subscribePaused);
  const inactive = paused || state?.activity === "suspended";
  const anchorStyle = {
    "--orb-anchor-x": `${Math.min(1, Math.max(0, anchor.x)) * 100}%`,
    "--orb-anchor-y": `${Math.min(1, Math.max(0, anchor.y)) * 100}%`,
  } as CSSProperties;

  return <main className={`floating-orb-root is-${surfacePhase} ${inactive ? "is-paused" : "is-live"}`} style={anchorStyle}>
    {expanded && <section className="floating-menu" aria-label="AirDrop 快捷菜单" onContextMenu={(event) => event.preventDefault()}>
      <header className="floating-menu-header" onPointerDown={startDragging}>
        <span className="floating-menu-mark"><FlowMark /></span>
        <span><strong>设备剪贴板</strong><small>{state?.slots.length ? `${state.slots.length} 台设备 · 拖动内容可直接插入` : "等待其他设备内容"}</small></span>
        <button ref={firstActionRef} type="button" aria-label="关闭快捷菜单" onPointerDown={(event) => event.stopPropagation()} onClick={beginCollapse}><Icon name="x" size={15} /></button>
      </header>
      <div className="floating-device-list">
        {state?.slots.length ? state.slots.map((slot) => <article className={`floating-device-item ${slot.online ? "is-online" : "is-offline"} ${draggingSlotId === slot.id ? "is-dragging" : ""}`} key={`${slot.id}-${slot.revision}`}>
          <span className="floating-device-icon"><Icon name={platformIcon[slot.platform]} size={17} /><span className="floating-device-presence" /><i><Icon name={kindIcon[slot.kind]} size={10} /></i></span>
          <span className="floating-device-copy" draggable={slot.available && usesDataTransfer(slot.kind) && Boolean(preparedDrags[dragKey(slot)])} onPointerDown={(event) => startFileDrag(event, slot)} onDragStart={(event) => startDataDrag(event, slot)} onDragEnd={finishDataDrag} title={slot.available ? "拖到目标应用，松开后直接插入" : undefined}>
            <span className="floating-device-title"><strong>{slot.deviceName}</strong><span className="floating-device-meta"><span className="floating-device-kind">{kindLabel[slot.kind]}</span><small>{slot.ageLabel}</small></span></span>
            {slot.imagePreview ? <img src={slot.imagePreview} alt={slot.preview} />
              : slot.fileNames?.length ? <span className="floating-file-names">{slot.fileNames.join(" · ")}</span>
                : <span className="floating-device-preview">{slot.preview}</span>}
          </span>
          <button type="button" disabled={!slot.available || usingSlotId !== null || draggingSlotId !== null} onPointerDown={(event) => event.stopPropagation()} onClick={() => runAction({ action: "use-slot", slotId: slot.id, revision: slot.revision })}>{usingSlotId === slot.id ? "取入中" : "使用"}</button>
        </article>) : <div className="floating-empty"><Icon name="devices" size={22} /><strong>暂无设备内容</strong><span>其他设备复制内容后会显示在这里</span></div>}
      </div>
      <div className="floating-status" role="status" aria-live="polite"><span />{status}</div>
      <footer className="floating-menu-actions">
        <button type="button" disabled={!state?.canReadClipboard || state.busy} onClick={() => runAction({ action: "publish-current" })}><Icon name="refresh" /><span>刷新</span></button>
        <button type="button" onClick={() => runAction({ action: "toggle-sync" })}><Icon name={paused ? "play" : "pause"} /><span>{paused ? "恢复" : "暂停"}</span></button>
        <button type="button" onClick={() => runAction({ action: "open-clipboard" })}><Icon name="clipboard" /><span>剪贴板</span></button>
        <button type="button" onClick={() => runAction({ action: "open-main" })}><Icon name="monitor" /><span>主窗口</span></button>
        <button type="button" onClick={() => runAction({ action: "hide-orb" })}><Icon name="x" /><span>隐藏</span></button>
      </footer>
    </section>}
    {(!expanded || surfacePhase === "opening" || surfacePhase === "closing") && <span className="floating-orb-anchor" aria-hidden={expanded || undefined}>
      <button
        className="floating-droplet"
        type="button"
        aria-label="AirDrop 悬浮球，左键拖动，右键打开菜单"
        title="左键拖动 · 右键打开菜单"
        tabIndex={expanded ? -1 : 0}
        disabled={Boolean(pendingLayout) || expanded}
        onPointerDown={startDragging}
        onContextMenu={openContextMenu}
      >
        <span className="floating-lobe floating-lobe-cyan" /><span className="floating-lobe floating-lobe-blue" />
        <span className="floating-liquid-edge" /><FlowMark />
      </button>
    </span>}
    {!expanded && <span className="sr-only" role="status" aria-live="polite">{status}</span>}
  </main>;
}
