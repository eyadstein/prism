import { describe, expect, it } from "vitest";
import { formatNm } from "./format";

describe("formatNm", () => {
  it("rounds to whole nanometres", () => {
    expect(formatNm(550.4)).toBe("550 nm");
  });

  it("handles the ends of the visible range", () => {
    expect(formatNm(380)).toBe("380 nm");
    expect(formatNm(780)).toBe("780 nm");
  });
});
