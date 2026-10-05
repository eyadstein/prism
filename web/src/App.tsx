import { formatNm } from "./lib/format";

const TICKS = [380, 450, 520, 580, 650, 780];

export default function App() {
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
      <ul className="ticks">
        {TICKS.map((nm) => (
          <li key={nm}>{formatNm(nm)}</li>
        ))}
      </ul>
    </main>
  );
}
