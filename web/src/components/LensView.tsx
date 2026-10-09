import { useEffect, useRef } from "react";
import {
  fitView,
  surfaceZ,
  toCanvas,
  type CanvasPoint,
  type DrawnSurface,
  type LensDrawing,
  type View,
} from "../lib/drawing";

export interface TracedDrawing {
  color: string;
  drawing: LensDrawing;
}

const WIDTH = 960;
const HEIGHT = 320;
const MARGIN = 24;
const STEPS = 32;

function tracePath(ctx: CanvasRenderingContext2D, points: CanvasPoint[]): void {
  points.forEach((p, k) => {
    if (k === 0) {
      ctx.moveTo(p.x, p.y);
    } else {
      ctx.lineTo(p.x, p.y);
    }
  });
}

function profile(surface: DrawnSurface, aperture: number, view: View): CanvasPoint[] {
  const points: CanvasPoint[] = [];
  for (let k = 0; k <= STEPS; k += 1) {
    const y = -aperture + (2 * aperture * k) / STEPS;
    points.push(toCanvas(view, surfaceZ(surface, y), y));
  }
  return points;
}

function paintLens(ctx: CanvasRenderingContext2D, drawing: LensDrawing, view: View): void {
  ctx.fillStyle = "rgba(214, 211, 200, 0.12)";
  ctx.strokeStyle = "#8d8f8a";
  ctx.lineWidth = 1.5;
  drawing.surfaces.forEach((surface, i) => {
    ctx.beginPath();
    tracePath(ctx, profile(surface, surface.aperture, view));
    ctx.stroke();
    const next = drawing.surfaces[i + 1];
    if (surface.glass && next) {
      const aperture = Math.min(surface.aperture, next.aperture);
      const front = profile(surface, aperture, view);
      const back = profile(next, aperture, view).reverse();
      ctx.beginPath();
      tracePath(ctx, [...front, ...back]);
      ctx.closePath();
      ctx.fill();
    }
  });
}

function paint(ctx: CanvasRenderingContext2D, traces: TracedDrawing[]): void {
  ctx.clearRect(0, 0, WIDTH, HEIGHT);
  const first = traces[0];
  if (!first) {
    return;
  }
  const view = fitView(
    traces.map((t) => t.drawing),
    WIDTH,
    HEIGHT,
    MARGIN,
  );

  ctx.strokeStyle = "#2a2d2b";
  ctx.lineWidth = 1;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  tracePath(ctx, [toCanvas(view, view.zMin, 0), toCanvas(view, view.zMax, 0)]);
  ctx.stroke();
  ctx.setLineDash([]);

  paintLens(ctx, first.drawing, view);

  const top = toCanvas(view, first.drawing.imageZ, 0);
  ctx.strokeStyle = "#6b6e69";
  ctx.beginPath();
  ctx.moveTo(top.x, MARGIN);
  ctx.lineTo(top.x, HEIGHT - MARGIN);
  ctx.stroke();

  ctx.lineWidth = 1.2;
  ctx.globalAlpha = 0.75;
  for (const trace of traces) {
    ctx.strokeStyle = trace.color;
    for (const ray of trace.drawing.rays) {
      ctx.beginPath();
      tracePath(
        ctx,
        ray.map((p) => toCanvas(view, p.z, p.y)),
      );
      ctx.stroke();
    }
  }
  ctx.globalAlpha = 1;
}

export default function LensView({ traces }: { traces: TracedDrawing[] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) {
      return;
    }
    paint(ctx, traces);
  }, [traces]);
  return (
    <canvas
      ref={ref}
      className="lens-view"
      width={WIDTH}
      height={HEIGHT}
      role="img"
      aria-label="Cross-section of the lens with rays traced at three wavelengths"
    />
  );
}