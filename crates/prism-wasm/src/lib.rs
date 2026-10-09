//! Browser bindings for the Prism engine.
//!
//! Each exported function is a thin wrapper around a plain Rust function that returns
//! `Result<_, String>`, so the logic is tested natively.

#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use prism_core::lens::{Lens, D_LINE_NM};
use prism_core::optimize::{optimize, rms_spot_radius, Problem};
use prism_core::render::{render, RenderSettings};
use prism_core::report::lens_report;
use prism_core::scenefile::SceneFile;
use wasm_bindgen::prelude::*;

const MAX_PIXELS: usize = 4_000_000;
const MAX_ITERATIONS: usize = 200;

fn render_rgba(text: &str, width: usize, height: usize, samples: u32) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || samples == 0 {
        return Err("width, height and samples must be at least 1".to_owned());
    }
    if width.saturating_mul(height) > MAX_PIXELS {
        return Err(format!("image too large (limit {MAX_PIXELS} pixels)"));
    }
    let file = SceneFile::parse(text).map_err(|e| e.to_string())?;
    let camera = file.camera.camera(width as f64 / height as f64);
    let settings = RenderSettings {
        width,
        height,
        samples,
        ..RenderSettings::default()
    };
    let image = render(&file.scene, &camera, &settings);
    let rgb = image.to_srgb8();
    let mut rgba = Vec::with_capacity(width * height * 4);
    for i in 0..width * height {
        rgba.extend_from_slice(&rgb[i * 3..i * 3 + 3]);
        rgba.push(255);
    }
    Ok(rgba)
}

fn analyze_text(text: &str, pupil: f64, field: f64) -> Result<String, String> {
    let lens = Lens::parse(text).map_err(|e| e.to_string())?;
    lens_report(&lens, "lens", pupil, field)
        .ok_or_else(|| "lens has no finite focal length".to_owned())
}

fn optimize_text(text: &str, iterations: usize) -> Result<String, String> {
    let lens = Lens::parse(text).map_err(|e| e.to_string())?;
    let start = lens
        .paraxial(D_LINE_NM)
        .ok_or_else(|| "lens has no finite focal length".to_owned())?;
    let vary = (0..lens.surfaces().len()).collect();
    let problem = Problem::new(lens.with_image_distance(start.bfd), vary, start.efl);
    let before = rms_spot_radius(&problem.lens, &problem) * 1000.0;
    let outcome = optimize(&problem, iterations.min(MAX_ITERATIONS)).map_err(|e| e.to_string())?;
    let after = rms_spot_radius(&outcome.lens, &problem) * 1000.0;
    Ok(format!(
        "# RMS spot radius {before:.2} um before, {after:.2} um after ({} iterations)\n{}",
        outcome.iterations,
        outcome.lens.to_prescription()
    ))
}

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

/// Renders a scene file (without `mesh` directives) to RGBA bytes, row by row, ready for
/// a canvas `ImageData`.
#[wasm_bindgen]
pub fn render_scene(text: &str, width: u32, height: u32, samples: u32) -> Result<Vec<u8>, JsError> {
    render_rgba(text, width as usize, height as usize, samples).map_err(|e| JsError::new(&e))
}

/// Analyzes a lens prescription and returns the text report.
#[wasm_bindgen]
pub fn analyze_lens(text: &str, pupil: f64, field: f64) -> Result<String, JsError> {
    analyze_text(text, pupil, field).map_err(|e| JsError::new(&e))
}

/// Optimizes a lens prescription and returns the improved prescription, with a comment
/// line on top that summarizes the improvement.
#[wasm_bindgen]
pub fn optimize_lens(text: &str, iterations: u32) -> Result<String, JsError> {
    optimize_text(text, iterations as usize).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = "camera 0 0 5 0 0 0 40\nsphere 0 0 0 1 mirror 0.5\n";
    const SINGLET: &str = include_str!("../../../examples/singlet.lens");

    #[test]
    fn renders_rgba_bytes() {
        let rgba = render_rgba(SCENE, 8, 6, 4).expect("renders");
        assert_eq!(rgba.len(), 8 * 6 * 4);
        assert!(rgba.iter().skip(3).step_by(4).all(|&a| a == 255));
        assert!(rgba.iter().step_by(4).any(|&r| r > 0));
    }

    #[test]
    fn render_errors_are_messages() {
        let missing = render_rgba("sphere 0 0 0 1 diffuse 0.5\n", 4, 4, 1).expect_err("no camera");
        assert!(missing.contains("camera"), "{missing}");
        assert!(render_rgba(SCENE, 0, 4, 1).is_err());
        assert!(render_rgba(SCENE, 4, 4, 0).is_err());
        let large = render_rgba(SCENE, 3000, 3000, 1).expect_err("too large");
        assert!(large.contains("too large"), "{large}");
    }

    #[test]
    fn analyzes_a_lens() {
        let text = include_str!("../../../examples/plano-convex.lens");
        let report = analyze_text(text, 5.0, 0.0).expect("analyzes");
        assert!(report.contains("effective focal length 100.000"));
        assert!(analyze_text("flat 5 N-BK7\nflat 10 air\n", 5.0, 0.0).is_err());
        assert!(analyze_text("1 2 unobtainium\n", 5.0, 0.0).is_err());
    }

    #[test]
    fn optimizes_a_lens() {
        let out = optimize_text(SINGLET, 5).expect("optimizes");
        assert!(out.starts_with("# RMS spot radius"));
        assert_eq!(Lens::parse(&out).expect("round trip").surfaces().len(), 2);
        assert!(optimize_text("flat 5 N-BK7\nflat 10 air\n", 5).is_err());
    }

    #[test]
    fn version_and_wavelength_checks() {
        assert_ne!(version(), "");
        assert!(check_wavelength(550.0).is_ok());
    }
}
