import { useRef, useState } from "react";
import { renderScene } from "../engine";
import { errorMessage } from "../lib/format";
import { DEFAULT_SCENE, SCENE_PRESETS } from "../lib/presets";

const QUALITY = [
  { label: "Draft (320 x 180, 16 spp)", width: 320, height: 180, samples: 16 },
  { label: "Standard (480 x 270, 48 spp)", width: 480, height: 270, samples: 48 },
  { label: "High (640 x 360, 128 spp)", width: 640, height: 360, samples: 128 },
];

export default function SceneTab() {
  const [text, setText] = useState(DEFAULT_SCENE);
  const [quality, setQuality] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const render = async () => {
    const q = QUALITY[quality];
    if (!q) {
      return;
    }
    setBusy(true);
    setError(null);
    const started = performance.now();
    try {
      const pixels = await renderScene(text, q.width, q.height, q.samples);
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext("2d");
      if (canvas && ctx) {
        canvas.width = q.width;
        canvas.height = q.height;
        ctx.putImageData(new ImageData(new Uint8ClampedArray(pixels), q.width, q.height), 0, 0);
        const seconds = ((performance.now() - started) / 1000).toFixed(1);
        setNote(`${q.width} x ${q.height}, ${q.samples} samples per pixel, ${seconds} s`);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel">
      <div className="column">
        <div className="toolbar">
          <label>
            Preset
            <select
              defaultValue={SCENE_PRESETS[0].name}
              onChange={(e) => {
                const preset = SCENE_PRESETS.find((p) => p.name === e.target.value);
                if (preset) {
                  setText(preset.text);
                }
              }}
            >
              {SCENE_PRESETS.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Quality
            <select value={quality} onChange={(e) => setQuality(Number(e.target.value))}>
              {QUALITY.map((q, i) => (
                <option key={q.label} value={i}>
                  {q.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <textarea
          aria-label="Scene description"
          spellCheck={false}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="toolbar">
          <button type="button" disabled={busy} onClick={() => void render()}>
            {busy ? "Rendering" : "Render"}
          </button>
        </div>
        <p className="hint">
          Rendering runs in a background thread on one core, so the page stays responsive but a
          render takes longer than the command line tool. Scenes that load mesh files need
          `prism render`.
        </p>
      </div>
      <div className="column">
        <canvas
          ref={canvasRef}
          className="render-view"
          width={320}
          height={180}
          role="img"
          aria-label="Rendered scene"
        />
        {note !== "" && <p className="hint">{note}</p>}
        {error !== null && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </div>
    </section>
  );
}