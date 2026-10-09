/** Messages exchanged with the engine worker. */
export type EngineRequest =
  | { id: number; kind: "analyze"; text: string; pupil: number; field: number }
  | { id: number; kind: "optimize"; text: string; iterations: number }
  | {
      id: number;
      kind: "drawing";
      text: string;
      nm: number;
      field: number;
      rays: number;
      pupil: number;
    }
  | { id: number; kind: "render"; text: string; width: number; height: number; samples: number };

export type EngineReply =
  | { id: number; ok: true; value: string | Float64Array | Uint8Array }
  | { id: number; ok: false; error: string };