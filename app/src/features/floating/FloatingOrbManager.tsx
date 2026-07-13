import { useEffect, useMemo, useRef } from "react";
import type { PageId, UiSnapshot } from "../../model";
import type { DesktopClient } from "../../ipc/client";
import {
  FloatingOrbReconciler,
  createFloatingAdapter,
  type FloatingAdapter,
} from "./floating-adapter";
import {
  FLOATING_EVENTS,
  type FloatingEventPayloads,
  type FloatingOrbActionPayload,
  type FloatingOrbLayoutPayload,
  type FloatingOrbStatePayload,
} from "./floating-events";
import {
  COLLAPSED_ORB_SIZE,
  clampedRect,
  horizontalFractionForRect,
  resizedRect,
  sameRect,
  sideForRect,
  snappedRect,
  verticalFractionForRect,
  type FloatingPlacement,
} from "./floating-geometry";

export const FLOATING_SIDE_STORAGE_KEY = "airdrop.floating.side.v1";
export const FLOATING_HORIZONTAL_STORAGE_KEY = "airdrop.floating.horizontal.v1";
export const FLOATING_VERTICAL_STORAGE_KEY = "airdrop.floating.vertical.v1";

export const readFloatingPlacement = (): FloatingPlacement => {
  if (typeof window === "undefined") return { side: "right", horizontalFraction: 1, verticalFraction: 0.5 };
  try {
    const side = window.localStorage.getItem(FLOATING_SIDE_STORAGE_KEY);
    const storedHorizontalFraction = window.localStorage.getItem(FLOATING_HORIZONTAL_STORAGE_KEY);
    const storedFraction = window.localStorage.getItem(FLOATING_VERTICAL_STORAGE_KEY);
    const horizontalFraction = storedHorizontalFraction === null ? Number.NaN : Number(storedHorizontalFraction);
    const fraction = storedFraction === null ? Number.NaN : Number(storedFraction);
    const normalizedSide = side === "left" ? "left" : "right";
    return {
      side: normalizedSide,
      horizontalFraction: Number.isFinite(horizontalFraction) ? Math.min(1, Math.max(0, horizontalFraction)) : normalizedSide === "left" ? 0 : 1,
      verticalFraction: Number.isFinite(fraction) ? Math.min(1, Math.max(0, fraction)) : 0.5,
    };
  } catch {
    return { side: "right", horizontalFraction: 1, verticalFraction: 0.5 };
  }
};

const savePlacement = (placement: FloatingPlacement): void => {
  try {
    window.localStorage.setItem(FLOATING_SIDE_STORAGE_KEY, placement.side);
    window.localStorage.setItem(FLOATING_HORIZONTAL_STORAGE_KEY, String(placement.horizontalFraction ?? (placement.side === "left" ? 0 : 1)));
    window.localStorage.setItem(FLOATING_VERTICAL_STORAGE_KEY, String(placement.verticalFraction));
  } catch {
    // Window placement persistence is best-effort.
  }
};

export const snapshotToFloatingState = (snapshot: UiSnapshot): FloatingOrbStatePayload => ({
  publishPaused: snapshot.publishPaused,
  subscribePaused: snapshot.subscribePaused,
  activity: snapshot.activity,
  canReadClipboard: snapshot.clipboardCapability.canReadText,
  busy: snapshot.activity === "reconnecting" || snapshot.imports.some((item) => item.status === "fetching" || item.status === "committing"),
  slots: [...snapshot.slots]
    .sort((left, right) => Number(right.online) - Number(left.online) || right.sequence - left.sequence)
    .slice(0, 4)
    .map((slot) => ({
      id: slot.id,
      revision: slot.revision,
      deviceName: slot.deviceName,
      platform: slot.platform,
      kind: slot.representations.find((item) => item.enabled)?.kind ?? "private",
      preview: slot.preview,
      imagePreview: snapshot.settings.previewImages ? slot.imagePreview : undefined,
      fileNames: snapshot.settings.previewFileNames ? slot.fileNames : undefined,
      ageLabel: slot.ageLabel,
      available: (slot.availability === "ready" || slot.availability === "stale")
        && !snapshot.imports.some((item) => item.slotId === slot.id && (item.status === "fetching" || item.status === "committing")),
    })),
  appearance: {
    theme: snapshot.settings.theme,
    accentColor: snapshot.settings.accentColor,
    windowOpacity: snapshot.settings.windowOpacity,
    blurStrength: snapshot.settings.blurStrength,
    glassSaturation: snapshot.settings.glassSaturation,
    cornerRadius: snapshot.settings.cornerRadius,
    highlightStrength: snapshot.settings.highlightStrength,
  },
});

interface FloatingOrbManagerProps {
  client: DesktopClient;
  snapshot: UiSnapshot;
  setPage: (page: PageId) => void;
  onError: (message: string) => void;
  adapter?: FloatingAdapter;
}

const errorMessage = (error: unknown): string => error instanceof Error ? error.message : "悬浮球操作失败";

export function FloatingOrbManager({ client, snapshot, setPage, onError, adapter: suppliedAdapter }: FloatingOrbManagerProps) {
  const adapter = useMemo(() => suppliedAdapter ?? createFloatingAdapter(), [suppliedAdapter]);
  const reconciler = useMemo(() => new FloatingOrbReconciler(adapter), [adapter]);
  const readyRef = useRef(false);
  const snapshotRef = useRef(snapshot);
  const moveTimerRef = useRef<number | undefined>(undefined);
  const geometryQueueRef = useRef(Promise.resolve());
  const lifecycleGenerationRef = useRef(0);
  snapshotRef.current = snapshot;

  const transactGeometry = (operation: () => Promise<void>): Promise<void> => {
    geometryQueueRef.current = geometryQueueRef.current.catch(() => undefined).then(operation);
    return geometryQueueRef.current;
  };

  useEffect(() => {
    if (!adapter.supported || client.platform !== "desktop") return;
    const lifecycleGeneration = ++lifecycleGenerationRef.current;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const isCurrent = () => !disposed && lifecycleGenerationRef.current === lifecycleGeneration;

    const broadcastState = async () => {
      if (!disposed && readyRef.current) {
        await adapter.emit(FLOATING_EVENTS.state, snapshotToFloatingState(snapshotRef.current));
      }
    };

    const handleAction = async (payload: FloatingOrbActionPayload) => {
      if (payload.action === "use-slot") {
        const importId = await client.createImportIntent(payload.slotId, payload.revision);
        await client.confirmImport(importId);
        await broadcastState();
        return;
      }
      const { action } = payload;
      switch (action) {
        case "open-main":
          await adapter.showMain();
          break;
        case "open-clipboard":
          setPage("clipboard");
          await adapter.showMain();
          break;
        case "publish-current":
          await client.publishCurrentClipboard();
          break;
        case "toggle-sync": {
          const pauseBoth = !snapshotRef.current.publishPaused || !snapshotRef.current.subscribePaused;
          await client.setSynchronizationPaused(pauseBoth);
          break;
        }
        case "hide-orb":
          await client.updateSettings({ floatingOrbEnabled: false });
          await reconciler.reconcile(false);
          break;
      }
    };

    const handleLayout = async ({ requestId, expanded, width, height }: FloatingOrbLayoutPayload) => {
      await transactGeometry(async () => {
        try {
          const [current, workArea] = await Promise.all([adapter.getOrbBounds(), adapter.getOrbWorkArea()]);
          const placement = readFloatingPlacement();
          const side = sideForRect(current, workArea) ?? placement.side;
          const bounds = resizedRect(current, workArea, side, expanded, { width, height });
          await adapter.setOrbBounds(bounds);
          await adapter.emit(FLOATING_EVENTS.layoutState, { requestId, expanded, success: true, side, bounds });
        } catch (error) {
          const message = errorMessage(error);
          await adapter.emit(FLOATING_EVENTS.layoutState, { requestId, expanded, success: false, message });
          throw error;
        }
      });
    };

    const persistAfterMove = () => {
      window.clearTimeout(moveTimerRef.current);
      moveTimerRef.current = window.setTimeout(() => {
        void transactGeometry(async () => {
          const [current, workArea] = await Promise.all([adapter.getOrbBounds(), adapter.getOrbWorkArea()]);
          const bounds = clampedRect(current, workArea);
          const placement = {
            side: sideForRect(bounds, workArea),
            horizontalFraction: horizontalFractionForRect(bounds, workArea),
            verticalFraction: verticalFractionForRect(bounds, workArea),
          };
          savePlacement(placement);
          if (!sameRect(current, bounds)) await adapter.setOrbBounds(bounds);
        }).catch((error: unknown) => onError(errorMessage(error)));
      }, 180);
    };

    const register = async () => {
      const registerListener = async <K extends keyof FloatingEventPayloads>(
        event: K,
        handler: (payload: FloatingEventPayloads[K]) => void,
      ): Promise<boolean> => {
        const unlisten = await adapter.listen(event, handler);
        if (!isCurrent()) {
          unlisten();
          return false;
        }
        unlisteners.push(unlisten);
        return true;
      };

      try {
        if (!await registerListener(FLOATING_EVENTS.ready, () => {
          readyRef.current = true;
          void broadcastState().catch((error: unknown) => onError(errorMessage(error)));
        })) return;
        if (!await registerListener(FLOATING_EVENTS.action, (payload) => {
          void handleAction(payload).catch((error: unknown) => onError(errorMessage(error)));
        })) return;
        if (!await registerListener(FLOATING_EVENTS.layout, (payload) => {
          void handleLayout(payload).catch((error: unknown) => onError(errorMessage(error)));
        })) return;

        if (!isCurrent()) return;
        await reconciler.reconcile(snapshot.settings.floatingOrbEnabled);
      } catch (error) {
        unlisteners.splice(0).forEach((unlisten) => unlisten());
        throw error;
      }

      if (!isCurrent() || !snapshot.settings.floatingOrbEnabled) return;
      try {
        const unlistenMoved = await adapter.onOrbMoved(persistAfterMove);
        if (disposed) unlistenMoved(); else unlisteners.push(unlistenMoved);
      } catch (error) {
        onError(errorMessage(error));
      }

      try {
        await transactGeometry(async () => {
          const workArea = await adapter.getOrbWorkArea();
          await adapter.setOrbBounds(snappedRect(workArea, COLLAPSED_ORB_SIZE, readFloatingPlacement()));
        });
      } catch (error) {
        onError(errorMessage(error));
      }
    };

    void register().catch(async (error: unknown) => {
      if (disposed) return;
      onError(errorMessage(error));
      if (snapshot.settings.floatingOrbEnabled) {
        await client.updateSettings({ floatingOrbEnabled: false }).catch((rollbackError: unknown) => onError(errorMessage(rollbackError)));
      }
    });

    return () => {
      disposed = true;
      lifecycleGenerationRef.current += 1;
      readyRef.current = false;
      window.clearTimeout(moveTimerRef.current);
      unlisteners.splice(0).forEach((unlisten) => unlisten());
      void reconciler.reconcile(false).catch((error: unknown) => onError(errorMessage(error)));
    };
  }, [adapter, client, onError, reconciler, setPage, snapshot.settings.floatingOrbEnabled]);

  useEffect(() => {
    if (!adapter.supported || client.platform !== "desktop" || !snapshot.settings.floatingOrbEnabled || !readyRef.current) return;
    void adapter.emit(FLOATING_EVENTS.state, snapshotToFloatingState(snapshot)).catch((error: unknown) => onError(errorMessage(error)));
  }, [adapter, client.platform, onError, snapshot]);

  return null;
}
