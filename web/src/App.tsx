import { useState } from "react";
import LensTab from "./components/LensTab";
import SceneTab from "./components/SceneTab";

type Tab = "lens" | "scene";

const TABS: { id: Tab; label: string }[] = [
  { id: "lens", label: "Lens" },
  { id: "scene", label: "Scene" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("lens");
  return (
    <main className="shell">
      <header>
        <h1>Prism</h1>
        <p>Spectral lens designer and light simulator.</p>
      </header>
      <div
        className="spectrum"
        role="img"
        aria-label="Visible spectrum from 380 to 780 nanometres"
      />
      <div className="tabs" role="tablist">
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "tab active" : "tab"}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div hidden={tab !== "lens"}>
        <LensTab />
      </div>
      <div hidden={tab !== "scene"}>
        <SceneTab />
      </div>
    </main>
  );
}