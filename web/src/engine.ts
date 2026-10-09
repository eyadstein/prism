import type { EngineReply, EngineRequest } from "./protocol";

type Value = string | Float64Array | Uint8Array;

interface Pending {
  resolve: (value: Value) => void;
  reject: (error: Error) => void;
}

const pending = new Map<number, Pending>();
let worker: Worker | null = null;
let nextId = 1;

function failAll(message: string): void {
  for (const entry of pending.values()) {
    entry.reject(new Error(message));
  }
  pending.clear();
}

function connect(): Worker {
  if (worker) {
    return worker;
  }
  const created = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  created.onmessage = (event: MessageEvent<EngineReply>) => {
    const reply = event.data;
    const entry = pending.get(reply.id);
    if (!entry) {
      return;
    }
    pending.delete(reply.id);
    if (reply.ok) {
      entry.resolve(reply.value);
    } else {
      entry.reject(new Error(reply.error));
    }
  };
  created.onerror = (event: ErrorEvent) => {
    failAll(event.message || "the engine worker failed to start");
  };
  worker = created;
  return created;
}

function send(make: (id: number) => EngineRequest): Promise<Value> {
  const id = nextId;
  nextId += 1;
  return new Promise<Value>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    connect().postMessage(make(id));
  });
}

function asText(value: Value): string {
  if (typeof value !== "string") {
    throw new Error("the engine returned an unexpected reply");
  }
  return value;
}

function asNumbers(value: Value): Float64Array {
  if (!(value instanceof Float64Array)) {
    throw new Error("the engine returned an unexpected reply");
  }
  return value;
}

function asBytes(value: Value): Uint8Array {
  if (!(value instanceof Uint8Array)) {
    throw new Error("the engine returned an unexpected reply");
  }
  return value;
}

/** Text report: focal length, spot sizes, distortion, field curvature, MTF. */
export async function analyzeLens(text: string, pupil: number, field: number): Promise<string> {
  return asText(await send((id) => ({ id, kind: "analyze", text, pupil, field })));
}

/** Optimized prescription, with a comment line that summarizes the improvement. */
export async function optimizeLens(text: string, iterations: number): Promise<string> {
  return asText(await send((id) => ({ id, kind: "optimize", text, iterations })));
}

/** Flat ray-trace data for one wavelength; read it with `parseDrawing`. */
export async function lensDrawing(
  text: string,
  nm: number,
  field: number,
  rays: number,
  pupil: number,
): Promise<Float64Array> {
  return asNumbers(await send((id) => ({ id, kind: "drawing", text, nm, field, rays, pupil })));
}

/** RGBA pixels of a rendered scene. */
export async function renderScene(
  text: string,
  width: number,
  height: number,
  samples: number,
): Promise<Uint8Array> {
  return asBytes(await send((id) => ({ id, kind: "render", text, width, height, samples })));
}