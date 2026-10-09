import { useEffect, useState } from "react";
import { analyzeLens, lensDrawing, optimizeLens } from "../engine";
import { parseDrawing } from "../lib/drawing";
import { clampNumber, errorMessage, formatNm } from "../lib/format";
import { DEFAULT_LENS, LENS_PRESETS } from "../lib/presets";
import LensView, { type TracedDrawing } from "./LensView";

const WAVELENGTHS = [
  { nm: 450, color: "#5aa9ff" },
  { nm: 550, color: "#6fcf6f" },
  { nm: 650, color: "#ff6a55" },
];
const RAYS = 11;

export default function LensTab() {
  const [text, setText] = useState(DEFAULT_LENS);
  const [previous, setPrevious] = useState<string | null>(null);
  const [field, setField] = useState(0);
  const [pupil, setPupil] = useState(5);
  const [traces, setTraces] = useState<TracedDrawing[]>([]);
  const [report, setReport] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      const jobs = WAVELENGTHS.map(async ({ nm, color }) => ({
        color,
        drawing: parseDrawing(await lensDrawing(text, nm, field, RAYS, pupil)),
      }));
      Promise.all([Promise.all(jobs), analyzeLens(text, pupil, field)])
        .then(([drawn, reportText]) => {
          if (cancelled) {
            return;
          }
          setTraces(drawn);
          setReport(reportText);
          setError(null);
        })
        .catch((e: unknown) => {
          if (!cancelled) {
            setError(errorMessage(e));
          }
        });
    }, 300);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [text, field, pupil]);

  const optimize = async () => {
    setBusy(true);
    try {
      const result = await optimizeLens(text, 60);
      setPrevious(text);
      setText(result);
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
              defaultValue={LENS_PRESETS[0].name}
              onChange={(e) => {
                const preset = LENS_PRESETS.find((p) => p.name === e.target.value);
                if (preset) {
                  setText(preset.text);
                  setPrevious(null);
                }
              }}
            >
              {LENS_PRESETS.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Field (deg)
            <input
              type="number"
              min={0}
              max={30}
              step={0.5}
              value={field}
              onChange={(e) => setField(clampNumber(e.target.valueAsNumber, 0, 30, 0))}
            />
          </label>
          <label>
            Pupil radius
            <input
              type="number"
              min={0.5}
              max={20}
              step={0.5}
              value={pupil}
              onChange={(e) => setPupil(clampNumber(e.target.valueAsNumber, 0.5, 20, 5))}
            />
          </label>
        </div>
        <textarea
          aria-label="Lens prescription"
          spellCheck={false}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="toolbar">
          <button type="button" disabled={busy} onClick={() => void optimize()}>
            {busy ? "Optimizing" : "Optimize"}
          </button>
          {previous !== null && (
            <button
              type="button"
              onClick={() => {
                setText(previous);
                setPrevious(null);
              }}
            >
              Undo optimize
            </button>
          )}
        </div>
        <p className="hint">
          One surface per line: radius, thickness, glass, aperture. Millimetres, light travels left
          to right, <code>flat</code> is a plane, <code>air</code> is no glass.
        </p>
      </div>
      <div className="column">
        <LensView traces={traces} />
        <ul className="legend">
          {WAVELENGTHS.map(({ nm, color }) => (
            <li key={nm}>
              <span className="swatch" style={{ background: color }} />
              {formatNm(nm)}
            </li>
          ))}
        </ul>
        {error !== null && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <pre className="report">{report}</pre>
      </div>
    </section>
  );
}