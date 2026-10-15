//! Wave aberration of a lens.
//!
//! A grid of rays crosses the entrance pupil. The optical path length of each ray (index times
//! distance, summed over every medium) is measured up to a reference sphere that is centred on the
//! chief ray's image point and passes through the chief ray's exit point. A ray on a perfect
//! spherical wavefront has the same path length as the chief ray, so the difference is the wave
//! aberration. It is fitted with Zernike polynomials, whose orthonormality turns the RMS wavefront
//! error into a sum of squared coefficients.
//!
//! The pupil is taken perpendicular to the field direction. The chief ray is the one through the
//! first vertex. The lens is used as given, so its last thickness fixes the image plane.

use crate::check_wavelength;
use crate::error::{PrismError, Result};
use crate::lens::Lens;
use crate::math::{Ray, Vec3};
use crate::zernike::{fit, rms as zernike_rms, strehl_marechal, MAX_TERMS};

/// Most grid cells across the pupil diameter.
pub const MAX_GRID: usize = 128;
const RAY_LEAD: f64 = 100.0;
const MM_PER_NM: f64 = 1.0e-6;
const MIN_RADIUS: f64 = 1.0e-9;

/// What to trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavefrontSettings {
    /// Wavelength in nanometres.
    pub nm: f64,
    /// Field angle in degrees.
    pub field_deg: f64,
    /// Entrance pupil radius in millimetres.
    pub pupil_radius: f64,
    /// Grid cells across the pupil diameter (4 to `MAX_GRID`).
    pub grid: usize,
    /// Number of Zernike terms to fit (1 to `MAX_TERMS`).
    pub terms: usize,
}

impl Default for WavefrontSettings {
    fn default() -> Self {
        Self {
            nm: 550.0,
            field_deg: 0.0,
            pupil_radius: 5.0,
            grid: 32,
            terms: 15,
        }
    }
}

/// A measured wavefront. Values are optical path differences in waves.
#[derive(Clone, Debug)]
pub struct Wavefront {
    /// Cells across the pupil diameter.
    pub grid: usize,
    /// Wavelength in nanometres.
    pub nm: f64,
    /// Path difference of each cell, row-major with row 0 at the bottom of the pupil. `None` for
    /// cells outside the pupil disc and for rays that were blocked.
    pub opd: Vec<Option<f64>>,
    /// Number of cells inside the pupil disc, that is, rays launched.
    pub launched: usize,
    /// Zernike coefficients in waves, Noll term 1 first.
    pub coefficients: Vec<f64>,
}

impl Wavefront {
    /// Number of rays that got through the lens.
    pub fn samples(&self) -> usize {
        self.opd.iter().flatten().count()
    }

    /// RMS of the path differences about their mean (piston removed), measured directly.
    pub fn rms_direct(&self) -> f64 {
        let values: Vec<f64> = self.opd.iter().flatten().copied().collect();
        if values.is_empty() {
            return 0.0;
        }
        let count = values.len() as f64;
        let mean = values.iter().sum::<f64>() / count;
        let spread: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
        (spread / count).sqrt()
    }

    /// Largest minus smallest path difference.
    pub fn peak_to_valley(&self) -> f64 {
        let (low, high) = self
            .opd
            .iter()
            .flatten()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                (lo.min(v), hi.max(v))
            });
        if low <= high {
            high - low
        } else {
            0.0
        }
    }

    /// RMS wavefront error from the Zernike coefficients, without piston and optionally without tilt.
    pub fn rms_fit(&self, remove_tilt: bool) -> f64 {
        zernike_rms(&self.coefficients, remove_tilt)
    }

    /// Strehl ratio by the Marechal approximation, from the RMS without piston and tilt (tilt only
    /// moves the image, it does not lower the peak).
    pub fn strehl(&self) -> f64 {
        strehl_marechal(self.rms_fit(true))
    }
}

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

/// Normalized pupil position `(x, y)` of the centre of cell `(row, col)` of a `grid` by `grid`
/// array. Both run from -1 to 1 across the pupil diameter, `x` along the columns and `y` along
/// the rows.
pub fn cell_center(grid: usize, row: usize, col: usize) -> (f64, f64) {
    let size = grid as f64;
    (
        (2 * col + 1) as f64 / size - 1.0,
        (2 * row + 1) as f64 / size - 1.0,
    )
}

fn validate(s: &WavefrontSettings) -> Result<()> {
    check_wavelength(s.nm)?;
    if !s.field_deg.is_finite() {
        return Err(invalid("field_deg", "must be a number"));
    }
    if !(s.pupil_radius.is_finite() && s.pupil_radius > 0.0) {
        return Err(invalid("pupil_radius", "must be a positive number"));
    }
    if !(4..=MAX_GRID).contains(&s.grid) {
        return Err(invalid(
            "grid",
            &format!("must be between 4 and {MAX_GRID}"),
        ));
    }
    if !(1..=MAX_TERMS).contains(&s.terms) {
        return Err(invalid(
            "terms",
            &format!("must be between 1 and {MAX_TERMS}"),
        ));
    }
    Ok(())
}

/// Optical path length up to the reference sphere (centre `center`, radius `radius`) along a ray
/// that leaves the last surface at `exit` after a path length `opl`. The sphere point nearest the
/// exit point is used, which may lie slightly behind it.
fn reference_total(exit: &Ray, opl: f64, center: Vec3, radius: f64, n_last: f64) -> Option<f64> {
    let direction = exit.dir.normalized();
    let offset = exit.origin - center;
    let b = offset.dot(direction);
    let discriminant = b * b - (offset.length_squared() - radius * radius);
    if discriminant < 0.0 {
        return None;
    }
    Some(opl + n_last * (-b - discriminant.sqrt()))
}

/// Measures the wavefront of `lens` with the image plane where the lens puts it.
pub fn wavefront(lens: &Lens, settings: &WavefrontSettings) -> Result<Wavefront> {
    validate(settings)?;
    let last = lens
        .surfaces()
        .last()
        .ok_or_else(|| invalid("lens", "has no surfaces"))?;
    if !(last.thickness.is_finite() && last.thickness > 0.0) {
        return Err(invalid(
            "lens",
            "the image plane must lie behind the last surface",
        ));
    }
    let n_last = last.index(settings.nm);
    let image_z: f64 = lens.surfaces().iter().map(|s| s.thickness).sum();

    let theta = settings.field_deg.to_radians();
    let dir = Vec3::new(0.0, theta.sin(), theta.cos());
    let across = Vec3::new(0.0, theta.cos(), -theta.sin());
    let blocked = || invalid("lens", "the chief ray does not get through the lens");

    let (chief_exit, chief_opl) = lens
        .trace_opl(Ray::new(dir * (-RAY_LEAD), dir), settings.nm)
        .map_err(|_| blocked())?;
    if chief_exit.dir.z <= 1.0e-12 {
        return Err(blocked());
    }
    let t = (image_z - chief_exit.origin.z) / chief_exit.dir.z;
    let center = chief_exit.at(t);
    let radius = (center - chief_exit.origin).length();
    if !(radius.is_finite() && radius > MIN_RADIUS) {
        return Err(blocked());
    }
    let chief_total =
        reference_total(&chief_exit, chief_opl, center, radius, n_last).ok_or_else(blocked)?;

    let size = settings.grid;
    let lambda = settings.nm * MM_PER_NM;
    let mut opd = vec![None; size * size];
    let mut points = Vec::new();
    let mut launched = 0;
    for row in 0..size {
        for col in 0..size {
            let (x, y) = cell_center(size, row, col);
            if x * x + y * y > 1.0 {
                continue;
            }
            launched += 1;
            let start = Vec3::new(1.0, 0.0, 0.0) * (x * settings.pupil_radius)
                + across * (y * settings.pupil_radius)
                - dir * RAY_LEAD;
            let total = lens
                .trace_opl(Ray::new(start, dir), settings.nm)
                .ok()
                .and_then(|(exit, opl)| reference_total(&exit, opl, center, radius, n_last));
            if let Some(total) = total {
                let waves = (total - chief_total) / lambda;
                opd[row * size + col] = Some(waves);
                points.push((x, y, waves));
            }
        }
    }
    let coefficients = fit(&points, settings.terms)?;
    Ok(Wavefront {
        grid: size,
        nm: settings.nm,
        opd,
        launched,
        coefficients,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::D_LINE_NM;
    use std::f64::consts::PI;

    fn plano() -> Lens {
        Lens::parse(include_str!("../../../examples/plano-convex.lens")).expect("valid lens")
    }

    fn focused(shift: f64) -> Lens {
        let lens = plano();
        let bfd = lens.paraxial(D_LINE_NM).expect("finite focus").bfd;
        lens.with_image_distance(bfd + shift)
    }

    fn settings(pupil: f64, grid: usize) -> WavefrontSettings {
        WavefrontSettings {
            nm: D_LINE_NM,
            field_deg: 0.0,
            pupil_radius: pupil,
            grid,
            terms: 15,
        }
    }

    fn run(shift: f64, s: &WavefrontSettings) -> Wavefront {
        wavefront(&focused(shift), s).expect("traces")
    }

    fn invalid_named(result: &Result<Wavefront>, wanted: &str) -> bool {
        matches!(result, Err(PrismError::InvalidParameter { name, .. }) if *name == wanted)
    }

    #[test]
    fn the_centre_cell_is_the_chief_ray() {
        let w = run(0.0, &settings(3.0, 15));
        let centre = w.opd[7 * 15 + 7].expect("the centre ray passes");
        assert!(centre.abs() < 1e-9, "centre OPD {centre}");
    }

    #[test]
    fn a_small_pupil_is_nearly_diffraction_limited() {
        let w = run(0.0, &settings(0.5, 16));
        assert_eq!(w.samples(), w.launched);
        assert!(w.launched > 150, "launched {}", w.launched);
        assert_eq!(w.coefficients.len(), 15);
        assert!(w.rms_direct() < 0.01, "rms {}", w.rms_direct());
        assert!(w.strehl() > 0.999, "strehl {}", w.strehl());
    }

    #[test]
    fn spherical_aberration_scales_with_the_fourth_power_of_the_pupil() {
        let rms = |pupil: f64| run(0.0, &settings(pupil, 16)).rms_direct();
        let (small, large) = (rms(2.0), rms(4.0));
        assert!(small > 1e-5, "small {small}");
        let ratio = large / small;
        assert!((13.0..19.0).contains(&ratio), "ratio {ratio}");
    }

    #[test]
    fn the_fit_agrees_with_the_direct_rms_for_a_mild_wavefront() {
        let w = run(0.0, &settings(2.0, 32));
        let direct = w.rms_direct();
        let fitted = w.rms_fit(false);
        assert!(
            (fitted - direct).abs() < 0.1 * direct,
            "fit {fitted}, direct {direct}"
        );
    }

    #[test]
    fn moving_the_image_plane_changes_defocus_by_the_geometric_amount() {
        let s = WavefrontSettings {
            nm: 550.0,
            ..settings(0.5, 16)
        };
        let defocus = |shift: f64| run(shift, &s).coefficients[3];
        let difference = (defocus(2.0) - defocus(-2.0)).abs();
        let expected = 2.0 * 0.005_f64.powi(2) / 5.5e-4 / (2.0 * 3.0_f64.sqrt());
        assert!(
            (difference - expected).abs() < 0.05 * expected,
            "difference {difference}, expected {expected}"
        );
    }

    #[test]
    fn field_points_are_traced_too() {
        let s = WavefrontSettings {
            field_deg: 5.0,
            ..settings(0.5, 16)
        };
        let w = run(0.0, &s);
        assert_eq!(w.samples(), w.launched);
        assert!(w.rms_direct() < 0.08, "rms {}", w.rms_direct());
    }

    #[test]
    fn unusable_lenses_and_settings_are_rejected() {
        let lens = focused(0.0);
        let ok = settings(2.0, 8);
        let check = |s: WavefrontSettings, name: &str| invalid_named(&wavefront(&lens, &s), name);
        assert!(check(
            WavefrontSettings {
                pupil_radius: 0.0,
                ..ok
            },
            "pupil_radius"
        ));
        assert!(check(
            WavefrontSettings {
                field_deg: f64::NAN,
                ..ok
            },
            "field_deg"
        ));
        assert!(check(WavefrontSettings { grid: 3, ..ok }, "grid"));
        assert!(check(
            WavefrontSettings {
                grid: MAX_GRID + 1,
                ..ok
            },
            "grid"
        ));
        assert!(check(WavefrontSettings { terms: 0, ..ok }, "terms"));
        assert!(check(
            WavefrontSettings {
                terms: MAX_TERMS + 1,
                ..ok
            },
            "terms"
        ));
        assert!(check(
            WavefrontSettings {
                grid: 4,
                terms: 15,
                ..ok
            },
            "points"
        ));
        assert!(matches!(
            wavefront(&lens, &WavefrontSettings { nm: 200.0, ..ok }),
            Err(PrismError::WavelengthOutOfRange(_))
        ));

        let pinhole = Lens::parse("51.68 5.0 N-BK7 0.1\nflat 90 air 0.1\n").expect("valid lens");
        let tilted = WavefrontSettings {
            field_deg: 5.0,
            ..settings(0.05, 8)
        };
        assert!(invalid_named(&wavefront(&pinhole, &tilted), "lens"));

        let flush = Lens::parse("51.68 5.0 N-BK7 12.5\nflat 0 air 12.5\n").expect("valid lens");
        assert!(invalid_named(&wavefront(&flush, &settings(2.0, 8)), "lens"));
    }

    #[test]
    fn summary_numbers_on_a_known_wavefront() {
        let w = Wavefront {
            grid: 2,
            nm: 550.0,
            opd: vec![Some(0.1), Some(0.3), None, Some(0.2)],
            launched: 4,
            coefficients: vec![0.2, 0.0, 0.0, 0.07, 0.0],
        };
        assert_eq!(w.samples(), 3);
        assert!((w.peak_to_valley() - 0.2).abs() < 1e-12);
        assert!((w.rms_direct() - (0.02_f64 / 3.0).sqrt()).abs() < 1e-12);
        assert!((w.rms_fit(false) - 0.07).abs() < 1e-12);
        assert!((w.rms_fit(true) - 0.07).abs() < 1e-12);
        let expected = (-(2.0 * PI * 0.07).powi(2)).exp();
        assert!((w.strehl() - expected).abs() < 1e-12);

        let empty = Wavefront {
            opd: vec![None; 4],
            launched: 0,
            coefficients: Vec::new(),
            ..w.clone()
        };
        assert_eq!(empty.samples(), 0);
        assert!(empty.rms_direct().abs() < 1e-15);
        assert!(empty.peak_to_valley().abs() < 1e-15);
    }

    #[test]
    fn cell_centres_span_the_pupil() {
        assert_eq!(cell_center(4, 0, 0), (-0.75, -0.75));
        assert_eq!(cell_center(4, 3, 3), (0.75, 0.75));
        assert_eq!(cell_center(15, 7, 7), (0.0, 0.0));
        let (x, y) = cell_center(4, 1, 2);
        assert!((x - 0.25).abs() < 1e-15 && (y + 0.25).abs() < 1e-15);
    }
}
