import { Channel, invoke } from "@tauri-apps/api/core";
import type { RepresentationKind } from "../../model";

export type NativeDragItem = string[] | {
  data: string | Record<string, string>;
  types: string[];
};

export interface PreparedSlotDrag {
  item: NativeDragItem;
  leaseId?: string;
}

export interface SlotDragRequest {
  slotId: string;
  revision: number;
  deviceName: string;
  kind: RepresentationKind;
  preview: string;
}

export interface SlotDragResult {
  result: string;
  cursorPos?: { x: number; y: number };
}

export interface FloatingContentDragAdapter {
  readonly supported: boolean;
  prepare(slotId: string, revision: number): Promise<PreparedSlotDrag>;
  startFiles(request: SlotDragRequest, prepared: PreparedSlotDrag, onEvent: (event: SlotDragResult) => void): Promise<void>;
}

const FALLBACK_DRAG_ICON = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL7WQAAAABJRU5ErkJggg==";

const dragIcon = (request: SlotDragRequest): string => {
  try {
    const scale = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
    const canvas = document.createElement("canvas");
    canvas.width = 232 * scale;
    canvas.height = 66 * scale;
    const context = canvas.getContext("2d");
    if (!context) return FALLBACK_DRAG_ICON;
    context.scale(scale, scale);
    const gradient = context.createLinearGradient(0, 0, 232, 66);
    gradient.addColorStop(0, "#21bbae");
    gradient.addColorStop(0.58, "#208bc2");
    gradient.addColorStop(1, "#3074d2");
    context.fillStyle = gradient;
    context.beginPath();
    context.moveTo(16, 0);
    context.lineTo(216, 0);
    context.quadraticCurveTo(232, 0, 232, 16);
    context.lineTo(232, 50);
    context.quadraticCurveTo(232, 66, 216, 66);
    context.lineTo(16, 66);
    context.quadraticCurveTo(0, 66, 0, 50);
    context.lineTo(0, 16);
    context.quadraticCurveTo(0, 0, 16, 0);
    context.fill();
    context.fillStyle = "rgba(255,255,255,.18)";
    context.beginPath();
    context.arc(30, 33, 18, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#fff";
    context.font = "600 13px system-ui, sans-serif";
    context.fillText(request.deviceName.slice(0, 20), 57, 27, 160);
    context.fillStyle = "rgba(255,255,255,.82)";
    context.font = "11px system-ui, sans-serif";
    const summary = request.kind === "files" ? "文件" : request.kind === "image" ? "图片" : request.preview;
    context.fillText(summary.slice(0, 28), 57, 46, 160);
    context.strokeStyle = "rgba(255,255,255,.72)";
    context.lineWidth = 2;
    context.beginPath();
    context.moveTo(23, 33);
    context.lineTo(36, 33);
    context.moveTo(31, 27);
    context.lineTo(37, 33);
    context.lineTo(31, 39);
    context.stroke();
    return canvas.toDataURL("image/png");
  } catch {
    return FALLBACK_DRAG_ICON;
  }
};

const browserAdapter: FloatingContentDragAdapter = {
  supported: false,
  prepare: async () => {
    throw new Error("当前环境不支持跨应用拖放");
  },
  startFiles: async () => {
    throw new Error("当前环境不支持跨应用拖放");
  },
};

const tauriAdapter: FloatingContentDragAdapter = {
  supported: true,
  prepare: (slotId, revision) => invoke<PreparedSlotDrag>("prepare_slot_drag", { slotId, revision }),
  startFiles: async (request, prepared, onEvent) => {
    if (!Array.isArray(prepared.item)) throw new Error("此内容应使用数据拖放");
    let releaseRequested = false;
    const release = async () => {
      if (!prepared.leaseId || releaseRequested) return;
      releaseRequested = true;
      await invoke("release_slot_drag", { leaseId: prepared.leaseId });
    };
    const onEventChannel = new Channel<SlotDragResult>((event) => {
      void release();
      onEvent(event);
    });
    try {
      await invoke("plugin:drag|start_drag", {
        item: prepared.item,
        image: dragIcon(request),
        options: { mode: "copy" },
        onEvent: onEventChannel,
      });
    } catch (error) {
      await release().catch(() => undefined);
      throw error;
    }
  },
};

export const createFloatingContentDragAdapter = (): FloatingContentDragAdapter =>
  typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__) ? tauriAdapter : browserAdapter;
