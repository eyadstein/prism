/** Formats a wavelength in nanometres for display, e.g. `550 nm`. */
export function formatNm(nm: number): string {
  return `${nm.toFixed(0)} nm`;
}

/** Limits `value` to `[min, max]`, or returns `fallback` when it is not a finite number. */
export function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, value));
}

/** Readable text for anything that was thrown. */
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}