# Prism

A spectral light simulator and lens designer. Prism traces individual
wavelengths of light (380 to 780 nm) instead of faking colour with RGB, so
dispersion, thin-film interference, and lens aberrations come from the physics.

Status: under construction (scaffold stage).

## Layout

| Path | Purpose |
|---|---|
| `crates/prism-core` | Maths, materials, path tracer, lens tracer, optimizer |
| `crates/prism-cli` | `prism` command line tool |
| `crates/prism-wasm` | WebAssembly bindings for the browser |
| `web/` | React + TypeScript lens editor and viewer |
| `python/` | Denoiser training and analysis tools |

## Develop

cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npm install && npm run build && npm test
cd python && pip install -e ".[dev]" && pytest && ruff check .
