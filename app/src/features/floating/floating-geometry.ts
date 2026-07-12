import type { FloatingOrbSide } from "./floating-events";

export const COLLAPSED_ORB_SIZE = { width: 72, height: 68 } as const;
export const EXPANDED_ORB_SIZE = { width: 300, height: 76 } as const;

export interface FloatingRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FloatingPlacement {
  side: FloatingOrbSide;
  verticalFraction: number;
}

export const clamp = (value: number, minimum: number, maximum: number): number =>
  Math.min(Math.max(value, minimum), Math.max(minimum, maximum));

export const sideForRect = (rect: FloatingRect, workArea: FloatingRect): FloatingOrbSide =>
  rect.x + rect.width / 2 < workArea.x + workArea.width / 2 ? "left" : "right";

export const verticalFractionForRect = (rect: FloatingRect, workArea: FloatingRect): number => {
  const travel = Math.max(0, workArea.height - rect.height);
  return travel === 0 ? 0 : clamp((rect.y - workArea.y) / travel, 0, 1);
};

export const snappedRect = (
  workArea: FloatingRect,
  size: Pick<FloatingRect, "width" | "height">,
  placement: FloatingPlacement,
): FloatingRect => {
  const width = Math.min(size.width, workArea.width);
  const height = Math.min(size.height, workArea.height);
  return {
    x: placement.side === "left" ? workArea.x : workArea.x + workArea.width - width,
    y: workArea.y + clamp(placement.verticalFraction, 0, 1) * Math.max(0, workArea.height - height),
    width,
    height,
  };
};

export const resizedRect = (
  current: FloatingRect,
  workArea: FloatingRect,
  side: FloatingOrbSide,
  expanded: boolean,
): FloatingRect => {
  const target = expanded ? EXPANDED_ORB_SIZE : COLLAPSED_ORB_SIZE;
  const width = Math.min(target.width, workArea.width);
  const height = Math.min(target.height, workArea.height);
  const anchoredX = side === "right" ? current.x + current.width - width : current.x;
  return {
    x: clamp(anchoredX, workArea.x, workArea.x + workArea.width - width),
    y: clamp(current.y, workArea.y, workArea.y + workArea.height - height),
    width,
    height,
  };
};
