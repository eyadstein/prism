# Regenerates web/src/wasm: the engine compiled to WebAssembly plus JavaScript bindings.
# Run it again whenever the exported functions in crates/prism-wasm change.
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
  $lock = Get-Content Cargo.lock -Raw
  if ($lock -match 'name = "wasm-bindgen"\s+version = "([0-9.]+)"') { $version = $Matches[1] }
  else { throw 'wasm-bindgen not found in Cargo.lock' }
  $installed = ''
  if (Get-Command wasm-bindgen -ErrorAction SilentlyContinue) { $installed = (wasm-bindgen --version) -join '' }
  if ($installed -notlike "*$version*") {
    cargo install wasm-bindgen-cli --version $version --locked
    if ($LASTEXITCODE -ne 0) { throw 'could not install wasm-bindgen-cli' }
  }
  cargo build -p prism-wasm --target wasm32-unknown-unknown --release
  if ($LASTEXITCODE -ne 0) { throw 'wasm build failed' }
  wasm-bindgen --target web --out-dir web/src/wasm target/wasm32-unknown-unknown/release/prism_wasm.wasm
  if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen failed' }
} finally { Pop-Location }