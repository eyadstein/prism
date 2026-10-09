import bubble from "../../../examples/bubble.scene?raw";
import demo from "../../../examples/demo.scene?raw";
import doublet from "../../../examples/doublet.lens?raw";
import planoConvex from "../../../examples/plano-convex.lens?raw";
import singlet from "../../../examples/singlet.lens?raw";

export interface Preset {
  name: string;
  text: string;
}

export const DEFAULT_LENS = planoConvex;
export const DEFAULT_SCENE = demo;

export const LENS_PRESETS: Preset[] = [
  { name: "Plano-convex singlet", text: planoConvex },
  { name: "Biconvex singlet (unoptimized)", text: singlet },
  { name: "Achromatic doublet", text: doublet },
];

export const SCENE_PRESETS: Preset[] = [
  { name: "Glass spheres", text: demo },
  { name: "Soap bubbles", text: bubble },
];