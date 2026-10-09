/** A point in lens coordinates: `z` along the axis, `y` across it, both in millimetres. */
export interface Point {
  z: number;
  y: number;
}

export interface CanvasPoint {
  x: number;
  y: number;
}

export interface DrawnSurface {
  vertexZ: number;
  radius: number;
  aperture: number;
  glass: boolean;
}

export interface LensDrawing {
  imageZ: number;
  surfaces: DrawnSurface[];
  rays: Point[][];
}

/** Reads the flat array returned by the engine's `lens_drawing`. */
export function parseDrawing(data: Float64Array): LensDrawing {
  let i = 0;
  const next = (): number => {
    if (i >= data.length) {
      throw new Error("lens drawing data ended early");
    }
    const value = data[i];
    i += 1;
    return value;
  };
  const surfaceCount = Math.round(next());
  const imageZ = next();
  const surfaces: DrawnSurface[] = [];
  for (let s = 0; s < surfaceCount; s += 1) {
    const vertexZ = next();
    const radius = next();
    const aperture = next();
    const glass = next() > 0.5;
    surfaces.push({ vertexZ, radius, aperture, glass });
  }
  const rayCount = Math.round(next());
  const rays: Point[][] = [];
  for (let r = 0; r < rayCount; r += 1) {
    const pointCount = Math.round(next());
    const path: Point[] = [];
    for (let p = 0; p < pointCount; p += 1) {
      const z = next();
      const y = next();
      path.push({ z, y });
    }
    rays.push(path);
  }
  return { imageZ, surfaces, rays };
}

/** Axial position of a spherical surface at height `y`; flat surfaces return the vertex. */
export function surfaceZ(surface: DrawnSurface, y: number): number {
  const r = surface.radius;
  if (r === 0 || !Number.isFinite(r)) {
    return surface.vertexZ;
  }
  const h = Math.min(Math.abs(y), Math.abs(r));
  return surface.vertexZ + r - Math.sign(r) * Math.sqrt(r * r - h * h);
}

export interface View {
  zMin: number;
  zMax: number;
  scaleZ: number;
  scaleY: number;
  margin: number;
  height: number;
}

/** Chooses a scale that fits every drawing; the vertical scale is exaggerated at most 4 times. */
export function fitView(
  drawings: LensDrawing[],
  width: number,
  height: number,
  margin: number,
): View {
  let zMin = Number.POSITIVE_INFINITY;
  let zMax = Number.NEGATIVE_INFINITY;
  let yMax = 0;
  const includeZ = (z: number): void => {
    zMin = Math.min(zMin, z);
    zMax = Math.max(zMax, z);
  };
  for (const drawing of drawings) {
    includeZ(drawing.imageZ);
    for (const surface of drawing.surfaces) {
      includeZ(surface.vertexZ);
      if (Number.isFinite(surface.aperture)) {
        yMax = Math.max(yMax, surface.aperture);
      }
    }
    for (const ray of drawing.rays) {
      for (const point of ray) {
        includeZ(point.z);
        yMax = Math.max(yMax, Math.abs(point.y));
      }
    }
  }
  if (!Number.isFinite(zMin) || !Number.isFinite(zMax)) {
    zMin = 0;
    zMax = 1;
  }
  const zSpan = Math.max(zMax - zMin, 1e-9);
  const ySpan = Math.max(yMax, 1e-9);
  const scaleZ = (width - 2 * margin) / zSpan;
  const scaleY = Math.min((height - 2 * margin) / (2 * ySpan), scaleZ * 4);
  return { zMin, zMax, scaleZ, scaleY, margin, height };
}

/** Maps lens coordinates to canvas pixels (y grows downwards on a canvas). */
export function toCanvas(view: View, z: number, y: number): CanvasPoint {
  return {
    x: view.margin + (z - view.zMin) * view.scaleZ,
    y: view.height / 2 - y * view.scaleY,
  };
}