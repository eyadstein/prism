//! Browser bindings for the Prism engine.

use wasm_bindgen::prelude::*;

/// Returns the engine version string.
#[wasm_bindgen]
pub fn version() -> String {
    prism_core::VERSION.to_owned()
}

/// Validates a wavelength in nanometres, throwing a JS error when invalid.
#[wasm_bindgen]
pub fn check_wavelength(nm: f64) -> Result<f64, JsError> {
    prism_core::check_wavelength(nm).map_err(|e| JsError::new(&e.to_string()))
}
