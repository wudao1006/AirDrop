import { describe, expect, it } from "vitest";
import {
  COLLAPSED_ORB_SIZE,
  EXPANDED_ORB_SIZE,
  anchorForRect,
  clampedRect,
  horizontalFractionForRect,
  resizedRect,
  sideForRect,
  snappedRect,
  verticalFractionForRect,
} from "./floating-geometry";

describe("floating geometry", () => {
  const workArea = { x: 100, y: 40, width: 1000, height: 700 };

  it("snaps to the selected edge and clamps the vertical fraction", () => {
    expect(snappedRect(workArea, COLLAPSED_ORB_SIZE, { side: "right", verticalFraction: 2 })).toEqual({
      x: 1012,
      y: 656,
      ...COLLAPSED_ORB_SIZE,
    });
  });

  it("grows left from the right edge and remains inside the work area", () => {
    const expanded = resizedRect({ x: 1012, y: 720, ...COLLAPSED_ORB_SIZE }, workArea, "right", true);
    expect(expanded).toEqual({ x: 744, y: 320, ...EXPANDED_ORB_SIZE });
    expect(sideForRect(expanded, workArea)).toBe("right");
    expect(verticalFractionForRect(expanded, workArea)).toBe(1);
  });

  it("grows right from the left edge and shrinks oversized targets", () => {
    expect(resizedRect({ x: 100, y: 40, ...COLLAPSED_ORB_SIZE }, { x: 100, y: 40, width: 180, height: 60 }, "left", true)).toEqual({
      x: 100,
      y: 40,
      width: 180,
      height: 60,
    });
  });

  it("keeps a freely placed orb still unless it crosses the work area", () => {
    const current = { x: 460, y: 210, ...COLLAPSED_ORB_SIZE };
    expect(clampedRect(current, workArea)).toEqual(current);
    expect(horizontalFractionForRect(current, workArea)).toBeCloseTo(360 / 912);
    expect(clampedRect({ ...current, x: 1200 }, workArea)).toEqual({ ...current, x: 1012 });
  });

  it("keeps the visual transition anchored to the original orb center", () => {
    const anchor = anchorForRect(
      { x: 1012, y: 100, ...COLLAPSED_ORB_SIZE },
      { x: 744, y: 100, ...EXPANDED_ORB_SIZE },
    );
    expect(anchor.x).toBeCloseTo(312 / 356);
    expect(anchor.y).toBeCloseTo(42 / 420);
  });
});
