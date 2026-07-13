import { describe, expect, it } from "vitest";
import {
  EXPANDED_ORB_SIZE,
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
    expect(snappedRect(workArea, { width: 72, height: 68 }, { side: "right", verticalFraction: 2 })).toEqual({
      x: 1028,
      y: 672,
      width: 72,
      height: 68,
    });
  });

  it("grows left from the right edge and remains inside the work area", () => {
    const expanded = resizedRect({ x: 1028, y: 720, width: 72, height: 68 }, workArea, "right", true);
    expect(expanded).toEqual({ x: 744, y: 320, ...EXPANDED_ORB_SIZE });
    expect(sideForRect(expanded, workArea)).toBe("right");
    expect(verticalFractionForRect(expanded, workArea)).toBe(1);
  });

  it("grows right from the left edge and shrinks oversized targets", () => {
    expect(resizedRect({ x: 100, y: 40, width: 72, height: 68 }, { x: 100, y: 40, width: 180, height: 60 }, "left", true)).toEqual({
      x: 100,
      y: 40,
      width: 180,
      height: 60,
    });
  });

  it("keeps a freely placed orb still unless it crosses the work area", () => {
    const current = { x: 460, y: 210, width: 72, height: 68 };
    expect(clampedRect(current, workArea)).toEqual(current);
    expect(horizontalFractionForRect(current, workArea)).toBeCloseTo(360 / 928);
    expect(clampedRect({ ...current, x: 1200 }, workArea)).toEqual({ ...current, x: 1028 });
  });
});
