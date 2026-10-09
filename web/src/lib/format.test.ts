import { describe, expect, it } from "vitest";
import { clampNumber, errorMessage, formatNm } from "./format";

describe("formatNm", () => {
  it("rounds to whole nanometres", () => {
    expect(formatNm(550.4)).toBe("550 nm");
  });

  it("handles the ends of the visible range", () => {
    expect(formatNm(380)).toBe("380 nm");
    expect(formatNm(780)).toBe("780 nm");
  });
});

describe("clampNumber", () => {
  it("keeps values inside the range", () => {
    expect(clampNumber(5, 0, 10, 1)).toBe(5);
  });

  it("limits values outside the range", () => {
    expect(clampNumber(-3, 0, 10, 1)).toBe(0);
    expect(clampNumber(99, 0, 10, 1)).toBe(10);
  });

  it("falls back for values that are not numbers", () => {
    expect(clampNumber(Number.NaN, 0, 10, 7)).toBe(7);
    expect(clampNumber(Number.POSITIVE_INFINITY, 0, 10, 7)).toBe(7);
  });
});

describe("errorMessage", () => {
  it("reads errors and other values", () => {
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage("plain")).toBe("plain");
  });
});