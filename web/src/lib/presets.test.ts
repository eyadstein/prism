import { describe, expect, it } from "vitest";
import { DEFAULT_LENS, DEFAULT_SCENE, LENS_PRESETS, SCENE_PRESETS } from "./presets";

describe("presets", () => {
  it("have unique names and real content", () => {
    for (const list of [LENS_PRESETS, SCENE_PRESETS]) {
      expect(new Set(list.map((p) => p.name)).size).toBe(list.length);
      for (const preset of list) {
        expect(preset.text.length).toBeGreaterThan(20);
      }
    }
    expect(LENS_PRESETS).toHaveLength(3);
    expect(SCENE_PRESETS).toHaveLength(2);
  });

  it("lens presets name a glass and scene presets define a camera", () => {
    for (const preset of LENS_PRESETS) {
      expect(preset.text).toContain("N-BK7");
    }
    for (const preset of SCENE_PRESETS) {
      expect(preset.text).toContain("camera");
    }
  });

  it("defaults come from the presets", () => {
    expect(DEFAULT_LENS).toBe(LENS_PRESETS[0].text);
    expect(DEFAULT_SCENE).toBe(SCENE_PRESETS[0].text);
  });
});