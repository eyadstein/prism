import { describe, expect, it } from "vitest";
import { fitView, parseDrawing, surfaceZ, toCanvas } from "./drawing";

const DATA = Float64Array.from([
  2, 100,
  0, 50, 12, 1,
  5, 0, 12, 0,
  1,
  3, -10, 1, 0, 1, 100, 0,
]);

describe("parseDrawing", () => {
  it("reads surfaces and rays", () => {
    const drawing = parseDrawing(DATA);
    expect(drawing.imageZ).toBe(100);
    expect(drawing.surfaces).toHaveLength(2);
    expect(drawing.surfaces[0]).toEqual({ vertexZ: 0, radius: 50, aperture: 12, glass: true });
    expect(drawing.surfaces[1].glass).toBe(false);
    expect(drawing.rays).toHaveLength(1);
    expect(drawing.rays[0]).toHaveLength(3);
    expect(drawing.rays[0][2]).toEqual({ z: 100, y: 0 });
  });

  it("rejects truncated data", () => {
    expect(() => parseDrawing(DATA.slice(0, 5))).toThrow("ended early");
    expect(() => parseDrawing(new Float64Array(0))).toThrow("ended early");
  });
});

describe("surfaceZ", () => {
  const convex = { vertexZ: 2, radius: 50, aperture: 12, glass: true };

  it("follows the sphere", () => {
    expect(surfaceZ(convex, 0)).toBe(2);
    expect(surfaceZ(convex, 5)).toBeCloseTo(2.2506, 3);
    expect(surfaceZ(convex, -5)).toBeCloseTo(2.2506, 3);
  });

  it("curves the other way for a negative radius", () => {
    expect(surfaceZ({ ...convex, radius: -50 }, 5)).toBeCloseTo(2 - 0.2506, 3);
  });

  it("keeps flat surfaces flat", () => {
    expect(surfaceZ({ ...convex, radius: 0 }, 5)).toBe(2);
    expect(surfaceZ({ ...convex, radius: Number.POSITIVE_INFINITY }, 5)).toBe(2);
  });

  it("stays finite beyond the radius", () => {
    expect(surfaceZ({ ...convex, radius: 10 }, 100)).toBe(12);
  });
});

describe("fitView", () => {
  it("fits the whole drawing inside the margins", () => {
    const view = fitView([parseDrawing(DATA)], 960, 320, 24);
    expect(toCanvas(view, -10, 0).x).toBeCloseTo(24, 6);
    expect(toCanvas(view, 100, 0).x).toBeCloseTo(936, 6);
    expect(toCanvas(view, 0, 0).y).toBeCloseTo(160, 6);
    expect(toCanvas(view, 0, 12).y).toBeCloseTo(24, 6);
  });

  it("copes with an empty list", () => {
    const view = fitView([], 960, 320, 24);
    expect(Number.isFinite(view.scaleZ)).toBe(true);
    expect(Number.isFinite(view.scaleY)).toBe(true);
  });
});