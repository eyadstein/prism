import type { EngineReply, EngineRequest } from "./protocol";
import init, {
  analyze_lens,
  lens_drawing,
  optimize_lens,
  render_scene,
} from "./wasm/prism_wasm.js";

interface WorkerScope {
  onmessage: ((event: MessageEvent<EngineRequest>) => void) | null;
  postMessage(message: EngineReply): void;
}

const scope = self as unknown as WorkerScope;
const ready = init();

function run(request: EngineRequest): string | Float64Array | Uint8Array {
  switch (request.kind) {
    case "analyze":
      return analyze_lens(request.text, request.pupil, request.field);
    case "optimize":
      return optimize_lens(request.text, request.iterations);
    case "drawing":
      return lens_drawing(request.text, request.nm, request.field, request.rays, request.pupil);
    case "render":
      return render_scene(request.text, request.width, request.height, request.samples);
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

scope.onmessage = (event) => {
  const request = event.data;
  ready.then(
    () => {
      try {
        scope.postMessage({ id: request.id, ok: true, value: run(request) });
      } catch (error) {
        scope.postMessage({ id: request.id, ok: false, error: describe(error) });
      }
    },
    (error: unknown) => {
      scope.postMessage({ id: request.id, ok: false, error: describe(error) });
    },
  );
};