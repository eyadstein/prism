//! Browser bindings for the Prism engine.
//!
//! Each exported function is a thin wrapper around a plain Rust function that returns
//! `Result<_, String>`, so the logic is tested natively.

#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use prism_core::denoise::{denoise as denoise_image, DenoiseSettings};
use prism_core::features::compute as compute_features;
use prism_core::lens::{Lens, D_LINE_NM};
use prism_core::math::{Ray, Vec3};
use prism_core::optimize::{optimize, rms_spot_radius, Problem};
use prism_core::render::{render, RenderSettings};
use prism_core::report::lens_report;
use prism_core::scenefile::SceneFile;
use wasm_bindgen::prelude::*;

const MAX_PIXELS: usize = 4_000_000;
const MAX_ITERATIONS: usize = 200;
const MAX_DRAWN_RAYS: usize = 201;
const RAY_LEAD: f64 = 10.0;

fn render_rgba(
    text: &str,
    width: usize,
    height: usize,
    samples: u32,
    denoise: bool,
) -> Result<Vec<u8>, String> {
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
    let image = if denoise {
        let features = compute_features(&file.scene, &camera, width, height);
        denoise_image(&image, &features, &DenoiseSettings::default()).map_err(|e| e.to_string())?
    } else {
        image
    };
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

/// Flat layout: `[surface_count, image_z, (vertex_z, radius, aperture, glass) per surface,
/// ray_count, (point_count, (z, y) per point) per ray]`. The image plane sits at the d-line
/// paraxial focus, so rays of other wavelengths show chromatic aberration.
fn drawing_data(
    text: &str,
    nm: f64,
    field_deg: f64,
    rays: usize,
    pupil: f64,
) -> Result<Vec<f64>, String> {
    if rays == 0 || rays > MAX_DRAWN_RAYS {
        return Err(format!("rays must be between 1 and {MAX_DRAWN_RAYS}"));
    }
    if !(pupil.is_finite() && pupil > 0.0) {
        return Err("pupil radius must be positive".to_owned());
    }
    prism_core::check_wavelength(nm).map_err(|e| e.to_string())?;
    let lens = Lens::parse(text).map_err(|e| e.to_string())?;
    let focus = lens
        .paraxial(D_LINE_NM)
        .ok_or_else(|| "lens has no finite focal length".to_owned())?;
    if focus.bfd <= 0.0 {
        return Err("the focus lies inside the lens, so it cannot be drawn".to_owned());
    }
    let lens = lens.with_image_distance(focus.bfd);
    let image_z: f64 = lens.surfaces().iter().map(|s| s.thickness).sum();

    let theta = field_deg.to_radians();
    let dir = Vec3::new(0.0, theta.sin(), theta.cos());
    let mut out = vec![lens.surfaces().len() as f64, image_z];
    let mut vertex_z = 0.0;
    for s in lens.surfaces() {
        out.push(vertex_z);
        out.push(s.radius);
        out.push(s.semi_aperture.min(pupil * 1.5));
        out.push(if s.glass.is_some() { 1.0 } else { 0.0 });
        vertex_z += s.thickness;
    }
    let mut paths = Vec::new();
    for k in 0..rays {
        let u = if rays == 1 {
            0.0
        } else {
            (k as f64 / (rays - 1) as f64 * 2.0 - 1.0) * pupil
        };
        let start = Vec3::new(0.0, u, 0.0) - dir * RAY_LEAD;
        if let Ok(path) = lens.trace_path(Ray::new(start, dir), nm) {
            paths.push(path);
        }
    }
    out.push(paths.len() as f64);
    for path in &paths {
        out.push(path.len() as f64);
        for p in path {
            out.push(p.z);
            out.push(p.y);
        }
    }
    Ok(out)
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
/// a canvas `ImageData`. With `denoise` set, the image is filtered with the edge-avoiding
/// wavelet denoiser.
#[wasm_bindgen]
pub fn render_scene(
    text: &str,
    width: u32,
    height: u32,
    samples: u32,
    denoise: bool,
) -> Result<Vec<u8>, JsError> {
    render_rgba(text, width as usize, height as usize, samples, denoise)
        .map_err(|e| JsError::new(&e))
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

/// Traces `rays` rays across the pupil at wavelength `nm` and returns the surface layout
/// and ray paths for drawing a cross-section (see the layout in the source).
#[wasm_bindgen]
pub fn lens_drawing(
    text: &str,
    nm: f64,
    field_deg: f64,
    rays: u32,
    pupil: f64,
) -> Result<Vec<f64>, JsError> {
    drawing_data(text, nm, field_deg, rays as usize, pupil).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = "camera 0 0 5 0 0 0 40\nsphere 0 0 0 1 mirror 0.5\n";
    const SINGLET: &str = include_str!("../../../examples/singlet.lens");
    const PLANO: &str = include_str!("../../../examples/plano-convex.lens");

    #[test]
    fn renders_rgba_bytes() {
        let rgba = render_rgba(SCENE, 8, 6, 4, false).expect("renders");
        assert_eq!(rgba.len(), 8 * 6 * 4);
        assert!(rgba.iter().skip(3).step_by(4).all(|&a| a == 255));
        assert!(rgba.iter().step_by(4).any(|&r| r > 0));
    }

    #[test]
    fn denoised_render_has_the_same_shape() {
        let plain = render_rgba(SCENE, 8, 6, 4, false).expect("renders");
        let clean = render_rgba(SCENE, 8, 6, 4, true).expect("renders");
        assert_eq!(plain.len(), clean.len());
        assert!(clean.iter().skip(3).step_by(4).all(|&a| a == 255));
    }

    #[test]
    fn render_errors_are_messages() {
        let missing =
            render_rgba("sphere 0 0 0 1 diffuse 0.5\n", 4, 4, 1, false).expect_err("no camera");
        assert!(missing.contains("camera"), "{missing}");
        assert!(render_rgba(SCENE, 0, 4, 1, false).is_err());
        assert!(render_rgba(SCENE, 4, 4, 0, false).is_err());
        let large = render_rgba(SCENE, 3000, 3000, 1, false).expect_err("too large");
        assert!(large.contains("too large"), "{large}");
    }

    #[test]
    fn analyzes_a_lens() {
        let report = analyze_text(PLANO, 5.0, 0.0).expect("analyzes");
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
    fn drawing_has_the_documented_layout() {
        let d = drawing_data(PLANO, 550.0, 0.0, 5, 5.0).expect("draws");
        assert!((d[0] - 2.0).abs() < 1e-12);
        assert!(d[1] > 90.0, "image plane at {}", d[1]);
        assert!((d[2] - 0.0).abs() < 1e-12);
        assert!((d[3] - 51.68).abs() < 1e-9);
        assert!((d[4] - 7.5).abs() < 1e-9);
        assert!((d[5] - 1.0).abs() < 1e-12);
        assert!((d[6] - 5.0).abs() < 1e-12);
        assert!((d[10] - 5.0).abs() < 1e-12);
        assert!((d[11] - 4.0).abs() < 1e-12);
        assert_eq!(d.len(), 56);
    }

    #[test]
    fn drawing_rays_converge_on_the_image_plane() {
        let d = drawing_data(PLANO, 587.56, 0.0, 5, 5.0).expect("draws");
        let image_z = d[1];
        let end_z = d[11 + 1 + 6];
        let end_y = d[11 + 1 + 7];
        assert!((end_z - image_z).abs() < 1e-9);
        assert!(end_y.abs() < 0.5, "end y = {end_y}");
    }

    #[test]
    fn drawing_rejects_bad_requests() {
        assert!(drawing_data(PLANO, 550.0, 0.0, 0, 5.0).is_err());
        assert!(drawing_data(PLANO, 550.0, 0.0, 500, 5.0).is_err());
        assert!(drawing_data(PLANO, 550.0, 0.0, 5, 0.0).is_err());
        assert!(drawing_data(PLANO, 200.0, 0.0, 5, 5.0).is_err());
        assert!(drawing_data("flat 5 N-BK7\nflat 10 air\n", 550.0, 0.0, 5, 5.0).is_err());
        assert!(drawing_data("nonsense", 550.0, 0.0, 5, 5.0).is_err());
    }

    #[test]
    fn version_and_wavelength_checks() {
        assert_ne!(version(), "");
        assert!(check_wavelength(550.0).is_ok());
    }
}
