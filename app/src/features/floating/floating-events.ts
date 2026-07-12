import type { AppActivity } from "../../model";
import type { AppearanceSettings } from "../settings/appearance";

export const FLOATING_ORB_LABEL = "floating-orb";

export const FLOATING_EVENTS = {
  ready: "airdrop://orb-ready",
  state: "airdrop://orb-state",
  action: "airdrop://orb-action",
  layout: "airdrop://orb-layout",
  layoutState: "airdrop://orb-layout-state",
} as const;

export type FloatingOrbAction =
  | "open-main"
  | "open-clipboard"
  | "publish-current"
  | "toggle-sync"
  | "hide-orb";

export interface FloatingOrbReadyPayload {
  protocolVersion: 1;
}

export interface FloatingOrbStatePayload {
  publishPaused: boolean;
  subscribePaused: boolean;
  activity: AppActivity;
  canReadClipboard: boolean;
  busy: boolean;
  appearance: Pick<
    AppearanceSettings,
    "theme" | "accentColor" | "windowOpacity" | "blurStrength" | "glassSaturation" | "cornerRadius" | "highlightStrength"
  >;
}

export interface FloatingOrbActionPayload {
  action: FloatingOrbAction;
}

export interface FloatingOrbLayoutPayload {
  requestId: string;
  expanded: boolean;
}

export type FloatingOrbSide = "left" | "right";

export interface FloatingOrbLayoutStatePayload {
  requestId: string;
  expanded: boolean;
  success: boolean;
  message?: string;
  side?: FloatingOrbSide;
  bounds?: { x: number; y: number; width: number; height: number };
}

export interface FloatingEventPayloads {
  [FLOATING_EVENTS.ready]: FloatingOrbReadyPayload;
  [FLOATING_EVENTS.state]: FloatingOrbStatePayload;
  [FLOATING_EVENTS.action]: FloatingOrbActionPayload;
  [FLOATING_EVENTS.layout]: FloatingOrbLayoutPayload;
  [FLOATING_EVENTS.layoutState]: FloatingOrbLayoutStatePayload;
}
