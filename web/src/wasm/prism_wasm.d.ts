/* tslint:disable */
/* eslint-disable */

/**
 * Analyzes a lens prescription and returns the text report.
 */
export function analyze_lens(text: string, pupil: number, field: number): string;

/**
 * Validates a wavelength in nanometres, throwing a JS error when invalid.
 */
export function check_wavelength(nm: number): number;

/**
 * Traces `rays` rays across the pupil at wavelength `nm` and returns the surface layout
 * and ray paths for drawing a cross-section (see the layout in the source).
 */
export function lens_drawing(text: string, nm: number, field_deg: number, rays: number, pupil: number): Float64Array;

/**
 * Optimizes a lens prescription and returns the improved prescription, with a comment
 * line on top that summarizes the improvement.
 */
export function optimize_lens(text: string, iterations: number): string;

/**
 * Renders a scene file (without `mesh` directives) to RGBA bytes, row by row, ready for
 * a canvas `ImageData`. With `denoise` set, the image is filtered with the edge-avoiding
 * wavelet denoiser.
 */
export function render_scene(text: string, width: number, height: number, samples: number, denoise: boolean): Uint8Array;

/**
 * Returns the engine version string.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_lens: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly check_wavelength: (a: number) => [number, number, number];
    readonly lens_drawing: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly optimize_lens: (a: number, b: number, c: number) => [number, number, number, number];
    readonly render_scene: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly version: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
