//! Diffraction limits and through-focus MTF.
//!
//! Geometric ray tracing ignores diffraction, so its MTF is too optimistic for small apertures.
//! The diffraction-limited MTF of a circular aperture is known exactly, and the product of the
//! geometric MTF and the diffraction MTF is the standard first estimate of the real one. It is
//! only an estimate: it treats the two blurs as independent, which is accurate for small
//! aberrations and somewhat pessimistic for large ones.

use std::f64::consts::PI;

use crate::analysis::mtf_tangential;
use crate::error::{PrismError, Result};
use crate::lens::{Lens, D_LINE_NM};

/// Most focus positions one through-focus scan may evaluate.
pub const MAX_STEPS: usize = 400;
const SPOT_GRID: usize = 41;
/// Radius of the first dark ring of the Airy pattern in units of wavelength times f-number.
const FIRST_AIRY_ZERO: f64 = 1.219_669_891_266_648;

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// Working f-number `|efl| / (2 r)` of a lens whose entrance pupil has radius `pupil_radius`.
/// `None` for unusable inputs.
pub fn f_number(efl: f64, pupil_radius: f64) -> Option<f64> {
    if efl.is_finite() && efl != 0.0 && is_positive(pupil_radius) {
        Some(efl.abs() / (2.0 * pupil_radius))
    } else {
        None
    }
}

/// Spatial frequency in cycles per millimetre above which a diffraction-limited circular
/// aperture transmits no contrast: `1 / (wavelength * f-number)`. Zero for unusable inputs.
pub fn cutoff_frequency(nm: f64, f_number: f64) -> f64 {
    if is_positive(nm) && is_positive(f_number) {
        1.0e6 / (nm * f_number)
    } else {
        0.0
    }
}

/// Radius in micrometres of the first dark ring of the Airy pattern, `1.22 * wavelength * f-number`.
/// Zero for unusable inputs.
pub fn airy_radius_um(nm: f64, f_number: f64) -> f64 {
    if is_positive(nm) && is_positive(f_number) {
        FIRST_AIRY_ZERO * nm / 1000.0 * f_number
    } else {
        0.0
    }
}

/// Diffraction-limited MTF of a circular aperture at `cycles_per_mm`, for light of wavelength
/// `nm` and working f-number `f_number`: `(2 / pi) * (acos(x) - x * sqrt(1 - x^2))` with `x` the
/// frequency divided by the cutoff. It is 1 at zero frequency and 0 at and beyond the cutoff.
pub fn diffraction_mtf(cycles_per_mm: f64, nm: f64, f_number: f64) -> f64 {
    let cutoff = cutoff_frequency(nm, f_number);
    if cutoff == 0.0 || !cycles_per_mm.is_finite() {
        return 0.0;
    }
    let x = cycles_per_mm.abs() / cutoff;
    if x >= 1.0 {
        return 0.0;
    }
    2.0 / PI * (x.acos() - x * (1.0 - x * x).sqrt())
}

/// What to scan in a through-focus study.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusSettings {
    /// Wavelength in nanometres.
    pub nm: f64,
    /// Field angle in degrees.
    pub field_deg: f64,
    /// Entrance pupil radius in millimetres.
    pub pupil_radius: f64,
    /// Spatial frequency in cycles per millimetre.
    pub cycles_per_mm: f64,
    /// Largest shift from the paraxial focus to scan on each side, in millimetres.
    pub range: f64,
    /// Number of intervals between the two ends of the scan (2 to `MAX_STEPS`).
    pub steps: usize,
}

impl Default for FocusSettings {
    fn default() -> Self {
        Self {
            nm: 550.0,
            field_deg: 0.0,
            pupil_radius: 5.0,
            cycles_per_mm: 20.0,
            range: 2.0,
            steps: 20,
        }
    }
}

/// MTF at one image-plane position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusPoint {
    /// Offset of the image plane from the d-line paraxial focus, positive away from the lens.
    pub shift: f64,
    /// Tangential geometric MTF from the traced spot.
    pub geometric: f64,
    /// Geometric MTF multiplied by the diffraction-limited MTF.
    pub combined: f64,
}

fn validate(s: &FocusSettings) -> Result<()> {
    crate::check_wavelength(s.nm)?;
    if !s.field_deg.is_finite() {
        return Err(invalid("field_deg", "must be a number"));
    }
    if !is_positive(s.pupil_radius) {
        return Err(invalid("pupil_radius", "must be a positive number"));
    }
    if !(s.cycles_per_mm.is_finite() && s.cycles_per_mm >= 0.0) {
        return Err(invalid("cycles_per_mm", "must not be negative"));
    }
    if !is_positive(s.range) {
        return Err(invalid("range", "must be a positive number"));
    }
    if !(2..=MAX_STEPS).contains(&s.steps) {
        return Err(invalid(
            "steps",
            &format!("must be between 2 and {MAX_STEPS}"),
        ));
    }
    Ok(())
}

/// Moves the image plane through the d-line paraxial focus and records the MTF at each position.
/// Positions that would put the image plane inside the lens are skipped.
pub fn through_focus(lens: &Lens, settings: &FocusSettings) -> Result<Vec<FocusPoint>> {
    validate(settings)?;
    let focus = lens
        .paraxial(D_LINE_NM)
        .ok_or_else(|| invalid("lens", "has no finite focal length"))?;
    if !(focus.bfd.is_finite() && focus.bfd > 0.0) {
        return Err(invalid("lens", "the focus lies inside the lens"));
    }
    let number = f_number(focus.efl, settings.pupil_radius)
        .ok_or_else(|| invalid("lens", "has no usable f-number"))?;
    let diffraction = diffraction_mtf(settings.cycles_per_mm, settings.nm, number);
    let mut points = Vec::with_capacity(settings.steps + 1);
    for k in 0..=settings.steps {
        let shift = -settings.range + 2.0 * settings.range * k as f64 / settings.steps as f64;
        let distance = focus.bfd + shift;
        if distance <= 0.0 {
            continue;
        }
        let spot = lens.with_image_distance(distance).spot_diagram(
            settings.nm,
            settings.field_deg,
            SPOT_GRID,
            settings.pupil_radius,
        );
        let geometric = mtf_tangential(&spot, settings.cycles_per_mm);
        points.push(FocusPoint {
            shift,
            geometric,
            combined: geometric * diffraction,
        });
    }
    Ok(points)
}

fn peak_index(points: &[FocusPoint]) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.combined.total_cmp(&b.1.combined))
        .map(|(index, _)| index)
}

/// The scanned position with the highest combined MTF.
pub fn best_focus(points: &[FocusPoint]) -> Option<FocusPoint> {
    peak_index(points).map(|index| points[index])
}

/// The shifts `(low, high)` between which the combined MTF stays at least `fraction` of its peak,
/// found by walking outwards from the best focus over the scanned positions. The interval is
/// clipped to the scanned range. `None` when there is no positive peak or `fraction` is not in
/// `(0, 1]`.
pub fn depth_of_focus(points: &[FocusPoint], fraction: f64) -> Option<(f64, f64)> {
    if fraction.is_nan() || fraction <= 0.0 || fraction > 1.0 {
        return None;
    }
    let peak = peak_index(points)?;
    let level = fraction * points[peak].combined;
    if !level.is_finite() || level <= 0.0 {
        return None;
    }
    let mut low = peak;
    while low > 0 && points[low - 1].combined >= level {
        low -= 1;
    }
    let mut high = peak;
    while high + 1 < points.len() && points[high + 1].combined >= level {
        high += 1;
    }
    Some((points[low].shift, points[high].shift))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plano() -> Lens {
        Lens::parse(include_str!("../../../examples/plano-convex.lens")).expect("valid lens")
    }

    fn point(shift: f64, combined: f64) -> FocusPoint {
        FocusPoint {
            shift,
            geometric: combined,
            combined,
        }
    }

    fn rejected(settings: FocusSettings, expected: &str) -> bool {
        matches!(
            through_focus(&plano(), &settings),
            Err(PrismError::InvalidParameter { name, .. }) if name == expected
        )
    }

    #[test]
    fn known_diffraction_values() {
        let cutoff = cutoff_frequency(550.0, 2.0);
        assert!((cutoff - 909.090_909).abs() < 1e-3, "cutoff {cutoff}");
        assert!((diffraction_mtf(0.0, 550.0, 2.0) - 1.0).abs() < 1e-12);
        let half = diffraction_mtf(0.5 * cutoff, 550.0, 2.0);
        assert!((half - 0.391).abs() < 1e-3, "half-cutoff MTF {half}");
        assert!(diffraction_mtf(cutoff, 550.0, 2.0).abs() < 1e-12);
        assert!(diffraction_mtf(2.0 * cutoff, 550.0, 2.0).abs() < 1e-12);
        let mirrored = diffraction_mtf(-0.5 * cutoff, 550.0, 2.0);
        assert!((mirrored - half).abs() < 1e-15);
    }

    #[test]
    fn diffraction_mtf_only_falls() {
        let mut previous = 1.0;
        for k in 0..=200 {
            let value = diffraction_mtf(f64::from(k) * 5.0, 550.0, 2.0);
            assert!(value >= 0.0);
            assert!(value <= previous + 1e-12, "rose from {previous} to {value}");
            previous = value;
        }
    }

    #[test]
    fn airy_disc_and_cutoff_scale_with_the_f_number() {
        assert!((airy_radius_um(550.0, 2.0) - 1.3416).abs() < 1e-3);
        assert!((airy_radius_um(550.0, 4.0) - 2.0 * airy_radius_um(550.0, 2.0)).abs() < 1e-12);
        assert!((cutoff_frequency(550.0, 4.0) - 0.5 * cutoff_frequency(550.0, 2.0)).abs() < 1e-9);
        assert!(airy_radius_um(650.0, 2.0) > airy_radius_um(450.0, 2.0));
    }

    #[test]
    fn unusable_inputs_give_zero() {
        assert!(cutoff_frequency(-1.0, 2.0).abs() < 1e-15);
        assert!(cutoff_frequency(550.0, 0.0).abs() < 1e-15);
        assert!(airy_radius_um(550.0, f64::NAN).abs() < 1e-15);
        assert!(diffraction_mtf(10.0, 550.0, 0.0).abs() < 1e-15);
        assert!(diffraction_mtf(f64::NAN, 550.0, 2.0).abs() < 1e-15);
    }

    #[test]
    fn f_number_follows_the_definition() {
        assert_eq!(f_number(100.0, 5.0), Some(10.0));
        assert_eq!(f_number(-100.0, 5.0), Some(10.0));
        assert_eq!(f_number(100.0, 0.0), None);
        assert_eq!(f_number(0.0, 5.0), None);
        assert_eq!(f_number(f64::NAN, 5.0), None);
        assert_eq!(f_number(100.0, f64::INFINITY), None);
    }

    #[test]
    fn best_focus_and_depth_of_focus_on_a_known_curve() {
        let points = [
            point(-2.0, 0.1),
            point(-1.0, 0.4),
            point(0.0, 0.9),
            point(1.0, 0.5),
            point(2.0, 0.2),
        ];
        let best = best_focus(&points).expect("points exist");
        assert_eq!(best.shift, 0.0);
        assert_eq!(depth_of_focus(&points, 0.5), Some((0.0, 1.0)));
        assert_eq!(depth_of_focus(&points, 0.4), Some((-1.0, 1.0)));
        assert_eq!(depth_of_focus(&points, 1.0), Some((0.0, 0.0)));
        assert_eq!(depth_of_focus(&points, 0.01), Some((-2.0, 2.0)));
    }

    #[test]
    fn degenerate_depth_of_focus_requests_give_none() {
        let points = [point(-1.0, 0.2), point(0.0, 0.8), point(1.0, 0.3)];
        assert_eq!(depth_of_focus(&points, 0.0), None);
        assert_eq!(depth_of_focus(&points, 1.5), None);
        assert_eq!(depth_of_focus(&points, f64::NAN), None);
        assert_eq!(depth_of_focus(&[], 0.5), None);
        assert_eq!(best_focus(&[]), None);
        let flat = [point(-1.0, 0.0), point(1.0, 0.0)];
        assert_eq!(depth_of_focus(&flat, 0.5), None);
    }

    #[test]
    fn a_real_lens_peaks_near_focus_and_blurs_away_from_it() {
        let settings = FocusSettings {
            cycles_per_mm: 10.0,
            range: 3.0,
            steps: 12,
            ..FocusSettings::default()
        };
        let points = through_focus(&plano(), &settings).expect("scans");
        assert_eq!(points.len(), 13);
        assert!((points[0].shift + 3.0).abs() < 1e-12);
        assert!((points[12].shift - 3.0).abs() < 1e-12);
        let best = best_focus(&points).expect("points exist");
        assert!(best.combined > 3.0 * points[0].combined, "best {best:?}");
        assert!(best.combined > 3.0 * points[12].combined, "best {best:?}");
        assert!(best.shift.abs() < 1.5, "best focus at {}", best.shift);
        for p in &points {
            assert!(p.geometric >= 0.0 && p.geometric <= 1.0 + 1e-9, "{p:?}");
            assert!(
                p.combined >= 0.0 && p.combined <= p.geometric + 1e-12,
                "{p:?}"
            );
        }
        let (low, high) = depth_of_focus(&points, 0.5).expect("a peak exists");
        assert!(low <= best.shift && best.shift <= high);
    }

    #[test]
    fn small_apertures_are_diffraction_limited() {
        let settings = FocusSettings {
            pupil_radius: 0.5,
            cycles_per_mm: 5.0,
            range: 3.0,
            steps: 12,
            ..FocusSettings::default()
        };
        let points = through_focus(&plano(), &settings).expect("scans");
        let best = best_focus(&points).expect("points exist");
        let limit = diffraction_mtf(5.0, 550.0, 100.0);
        assert!(limit > 0.5 && limit < 0.8, "limit {limit}");
        assert!(
            (best.combined - limit).abs() < 0.02,
            "combined {} versus diffraction limit {limit}",
            best.combined
        );
        assert!(best.geometric > 0.95, "geometric {}", best.geometric);
    }

    #[test]
    fn bad_settings_are_rejected() {
        let ok = FocusSettings::default();
        assert!(rejected(
            FocusSettings {
                pupil_radius: 0.0,
                ..ok
            },
            "pupil_radius"
        ));
        assert!(rejected(
            FocusSettings {
                field_deg: f64::NAN,
                ..ok
            },
            "field_deg"
        ));
        assert!(rejected(
            FocusSettings {
                cycles_per_mm: -1.0,
                ..ok
            },
            "cycles_per_mm"
        ));
        assert!(rejected(FocusSettings { range: 0.0, ..ok }, "range"));
        assert!(rejected(FocusSettings { steps: 1, ..ok }, "steps"));
        assert!(rejected(
            FocusSettings {
                steps: MAX_STEPS + 1,
                ..ok
            },
            "steps"
        ));
        assert!(matches!(
            through_focus(&plano(), &FocusSettings { nm: 200.0, ..ok }),
            Err(PrismError::WavelengthOutOfRange(_))
        ));
        let plate = Lens::parse("flat 5 N-BK7 12\nflat 10 air 12\n").expect("valid lens");
        assert!(matches!(
            through_focus(&plate, &ok),
            Err(PrismError::InvalidParameter { name: "lens", .. })
        ));
    }
}
