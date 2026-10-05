/** Formats a wavelength in nanometres for display, e.g. `550 nm`. */
export function formatNm(nm: number): string {
  return `${nm.toFixed(0)} nm`;
}
