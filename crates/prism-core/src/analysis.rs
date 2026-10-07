//! Aberration analysis: distortion, field curvature (best focus shifts) and geometric MTF.

use std::f64::consts::TAU;

use crate::lens::{Lens, Spot};
use crate::math::{Ray, Vec3};

/// Aberrations at one field angle, measured on the lens's current image plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldPoint {
    /// Field angle in degrees.
    pub field_deg: f64,
    /// Image height of a distortion-free lens, `efl * tan(field)`.
    pub ideal_height: f64,
    /// Image height of the real chief ray (the ray through the first vertex).
    pub real_height: f64,
    /// `(real - ideal) / ideal` in percent; zero on axis.
    pub distortion_percent: f64,
    /// Offset of the best tangential focus from the image plane, positive away from the lens.
    pub tangential_focus: f64,
    /// Offset of the best sagittal focus from the image plane, positive away from the lens.
    pub sagittal_focus: f64,
}

#[derive(Clone, Copy)]
enum Fan {
    Tangential,
    Sagittal,
}

/// Where a ray crosses the image plane, and its direction there.
fn at_image(lens: &Lens, ray: Ray, nm: f64) -> Option<(Vec3, Vec3)> {
    let out = lens.trace(ray, nm).ok()?;
    if out.dir.z <= 0.0 {
        return None;
    }
    let image_z: f64 = lens.surfaces().iter().map(|s| s.thickness).sum();
    let t = (image_z - out.origin.z) / out.dir.z;
    Some((out.at(t), out.dir))
}

/// Traces a line of rays across the pupil and returns `(position, slope)` at the image plane.
fn fan_samples(
    lens: &Lens,
    nm: f64,
    dir: Vec3,
    pupil_radius: f64,
    rays: usize,
    fan: Fan,
) -> Vec<(f64, f64)> {
    let mut samples = Vec::new();
    for k in 0..rays {
        let u = ((k as f64 + 0.5) / rays as f64 * 2.0 - 1.0) * pupil_radius;
        let origin = match fan {
            Fan::Tangential => Vec3::new(0.0, u, 0.0),
            Fan::Sagittal => Vec3::new(u, 0.0, 0.0),
        };
        if let Some((p, d)) = at_image(lens, Ray::new(origin, dir), nm) {
            samples.push(match fan {
                Fan::Tangential => (p.y, d.y / d.z),
                Fan::Sagittal => (p.x, d.x / d.z),
            });
        }
    }
    samples
}

/// Shift of the image plane that minimizes the RMS spread of the rays. Each ray lands at
/// `a + b * shift`, so the spread is a quadratic in `shift` with a closed-form minimum.
fn best_focus_shift(samples: &[(f64, f64)]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let n = samples.len() as f64;
    let mean_a = samples.iter().map(|s| s.0).sum::<f64>() / n;
    let mean_b = samples.iter().map(|s| s.1).sum::<f64>() / n;
    let (mut num, mut den) = (0.0_f64, 0.0_f64);
    for &(a, b) in samples {
        num += (a - mean_a) * (b - mean_b);
        den += (b - mean_b) * (b - mean_b);
    }
    if den < 1e-30 {
        return None;
    }
    Some(-num / den)
}

/// Distortion and field curvature at `field_deg` degrees for wavelength `nm`, using `rays`
/// rays across a pupil of radius `pupil_radius`. Returns `None` when the chief ray or too
/// many fan rays are blocked, or the lens has no finite focal length.
pub fn field_point(
    lens: &Lens,
    nm: f64,
    field_deg: f64,
    pupil_radius: f64,
    rays: usize,
) -> Option<FieldPoint> {
    let efl = lens.paraxial(nm)?.efl;
    let theta = field_deg.to_radians();
    let dir = Vec3::new(0.0, theta.sin(), theta.cos());
    let (chief, _) = at_image(lens, Ray::new(Vec3::ZERO, dir), nm)?;
    let ideal = efl * theta.tan();
    let distortion_percent = if ideal.abs() < 1e-12 {
        0.0
    } else {
        (chief.y - ideal) / ideal * 100.0
    };
    let tangential = fan_samples(lens, nm, dir, pupil_radius, rays, Fan::Tangential);
    let sagittal = fan_samples(lens, nm, dir, pupil_radius, rays, Fan::Sagittal);
    Some(FieldPoint {
        field_deg,
        ideal_height: ideal,
        real_height: chief.y,
        distortion_percent,
        tangential_focus: best_focus_shift(&tangential)?,
        sagittal_focus: best_focus_shift(&sagittal)?,
    })
}

/// Geometric modulation transfer at `cycles_per_mm`: the magnitude of the Fourier transform
/// of the ray positions (in millimetres), normalized to 1 at zero frequency. It ignores
/// diffraction, so it is optimistic for small apertures.
pub fn mtf(positions: &[f64], cycles_per_mm: f64) -> f64 {
    if positions.is_empty() {
        return 0.0;
    }
    let (mut c, mut s) = (0.0_f64, 0.0_f64);
    for &x in positions {
        let phase = TAU * cycles_per_mm * x;
        c += phase.cos();
        s += phase.sin();
    }
    c.hypot(s) / positions.len() as f64
}

/// Tangential (y direction) geometric MTF of a spot diagram.
pub fn mtf_tangential(spot: &Spot, cycles_per_mm: f64) -> f64 {
    let ys: Vec<f64> = spot.points.iter().map(|p| p.1).collect();
    mtf(&ys, cycles_per_mm)
}

/// Sagittal (x direction) geometric MTF of a spot diagram.
pub fn mtf_sagittal(spot: &Spot, cycles_per_mm: f64) -> f64 {
    let xs: Vec<f64> = spot.points.iter().map(|p| p.0).collect();
    mtf(&xs, cycles_per_mm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::D_LINE_NM;

    fn base() -> Lens {
        Lens::parse("51.68 5.0 N-BK7 12.5\nflat 90 air 12.5\n").expect("valid lens")
    }

    fn bfd() -> f64 {
        base().paraxial(D_LINE_NM).expect("finite focus").bfd
    }

    fn focused() -> Lens {
        base().with_image_distance(bfd())
    }

    #[test]
    fn identical_positions_have_unit_mtf() {
        let same = [0.3; 10];
        for f in [0.0, 5.0, 50.0, 500.0] {
            assert!((mtf(&same, f) - 1.0).abs() < 1e-12);
        }
        assert!(mtf(&[], 10.0).abs() < 1e-30);
    }

    #[test]
    fn two_point_mtf_matches_cosine() {
        let d = 0.01;
        assert!(mtf(&[0.0, d], 50.0) < 1e-12);
        assert!((mtf(&[0.0, d], 100.0) - 1.0).abs() < 1e-12);
        assert!((mtf(&[0.0, d], 25.0) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((mtf(&[0.0, d], 0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn mtf_collapses_when_defocused() {
        let sharp = mtf_tangential(&focused().spot_diagram(D_LINE_NM, 0.0, 41, 5.0), 5.0);
        let blurred_lens = base().with_image_distance(bfd() + 5.0);
        let blurred = mtf_tangential(&blurred_lens.spot_diagram(D_LINE_NM, 0.0, 41, 5.0), 5.0);
        assert!(sharp > 0.5, "sharp = {sharp}");
        assert!(
            blurred < 0.3 * sharp,
            "blurred = {blurred}, sharp = {sharp}"
        );
    }

    #[test]
    fn on_axis_has_no_distortion_and_equal_fans() {
        let p = field_point(&focused(), D_LINE_NM, 0.0, 5.0, 41).expect("rays pass");
        assert!(p.real_height.abs() < 1e-9);
        assert_eq!(p.distortion_percent, 0.0);
        assert!((p.tangential_focus - p.sagittal_focus).abs() < 1e-9);
        assert!(
            p.tangential_focus < 0.0 && p.tangential_focus > -3.0,
            "shift = {}",
            p.tangential_focus
        );
    }

    #[test]
    fn distortion_is_small_and_grows_with_field() {
        let small = field_point(&focused(), D_LINE_NM, 0.5, 5.0, 41).expect("rays pass");
        let large = field_point(&focused(), D_LINE_NM, 5.0, 5.0, 41).expect("rays pass");
        assert!(large.real_height > 0.0);
        assert!((large.ideal_height - 100.0 * 5.0_f64.to_radians().tan()).abs() < 0.01);
        assert!(large.distortion_percent.abs() < 0.05);
        assert!(large.distortion_percent.abs() > small.distortion_percent.abs());
    }

    #[test]
    fn off_axis_focus_shifts_are_finite() {
        let p = field_point(&focused(), D_LINE_NM, 5.0, 5.0, 41).expect("rays pass");
        assert!(p.tangential_focus.abs() < 5.0);
        assert!(p.sagittal_focus.abs() < 5.0);
    }

    #[test]
    fn blocked_rays_give_none() {
        let pinhole = Lens::parse("51.68 5.0 N-BK7 0.1\nflat 90 air 0.1\n")
            .expect("valid lens")
            .with_image_distance(bfd());
        assert!(field_point(&pinhole, D_LINE_NM, 0.0, 5.0, 41).is_none());
    }
}
