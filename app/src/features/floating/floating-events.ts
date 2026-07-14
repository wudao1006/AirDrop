import type { AppActivity, RepresentationKind } from "../../model";
import type { AppearanceSettings } from "../settings/appearance";

export const FLOATING_ORB_LABEL = "floating-orb";

export const FLOATING_EVENTS = {
  ready: "airdrop://orb-ready",
  state: "airdrop://orb-state",
  action: "airdrop://orb-action",
  actionResult: "airdrop://orb-action-result",
  layout: "airdrop://orb-layout",
  layoutState: "airdrop://orb-layout-state",
  openMenu: "airdrop://orb-open-menu",
} as const;

export type FloatingOrbAction =
  | "open-main"
  | "open-clipboard"
  | "publish-current"
  | "toggle-sync"
  | "hide-orb";

export interface FloatingSlotSummary {
  id: string;
  revision: number;
  deviceName: string;
  platform: "macos" | "windows" | "linux" | "android";
  online: boolean;
  kind: RepresentationKind;
  preview: string;
  imagePreview?: string;
  fileNames?: string[];
  ageLabel: string;
  available: boolean;
}

export interface FloatingOrbReadyPayload {
  protocolVersion: 1;
}

export interface FloatingOrbStatePayload {
  publishPaused: boolean;
  subscribePaused: boolean;
  activity: AppActivity;
  canReadClipboard: boolean;
  busy: boolean;
  slots: FloatingSlotSummary[];
  appearance: Pick<
    AppearanceSettings,
    "theme" | "accentColor" | "windowOpacity" | "blurStrength" | "glassSaturation" | "cornerRadius" | "highlightStrength"
  >;
}

export type FloatingOrbActionCommand =
  | { action: FloatingOrbAction }
  | { action: "use-slot"; slotId: string; revision: number };

export type FloatingOrbActionPayload = FloatingOrbActionCommand & { requestId: string };

export interface FloatingOrbActionResultPayload {
  requestId: string;
  success: boolean;
  message: string;
}

export interface FloatingOrbLayoutPayload {
  requestId: string;
  expanded: boolean;
  width?: number;
  height?: number;
}

export type FloatingOrbSide = "left" | "right";

export interface FloatingOrbLayoutStatePayload {
  requestId: string;
  expanded: boolean;
  success: boolean;
  message?: string;
  side?: FloatingOrbSide;
  bounds?: { x: number; y: number; width: number; height: number };
  anchor?: { x: number; y: number };
}

export interface FloatingEventPayloads {
  [FLOATING_EVENTS.ready]: FloatingOrbReadyPayload;
  [FLOATING_EVENTS.state]: FloatingOrbStatePayload;
  [FLOATING_EVENTS.action]: FloatingOrbActionPayload;
  [FLOATING_EVENTS.actionResult]: FloatingOrbActionResultPayload;
  [FLOATING_EVENTS.layout]: FloatingOrbLayoutPayload;
  [FLOATING_EVENTS.layoutState]: FloatingOrbLayoutStatePayload;
  [FLOATING_EVENTS.openMenu]: Record<string, never>;
}
