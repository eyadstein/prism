//! Glass substitution scan.
//!
//! For every glass slot of a lens (the medium behind a surface), try every other glass in the
//! catalogue, re-optimize the curvatures and the image distance with the focal length held, and
//! rank the results by polychromatic RMS spot radius. The original lens is optimized with the same
//! budget first, so a swap only looks good when the glass itself is better and not because the
//! baseline was left unoptimized.

use crate::error::{PrismError, Result};
use crate::glass::catalog;
use crate::lens::{Lens, D_LINE_NM};
use crate::optimize::{optimize, rms_spot_radius, Problem};

/// Most optimizer iterations one candidate may use.
pub const MAX_ITERATIONS: usize = 100;
/// Materials that cannot be ground and cemented into a lens.
const NOT_USABLE: [&str; 2] = ["WATER", "DIAMOND"];

/// One glass swap after re-optimization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swap {
    /// Index of the surface whose medium was replaced.
    pub surface: usize,
    /// Glass before the swap.
    pub from: &'static str,
    /// Glass after the swap.
    pub to: &'static str,
    /// Effective focal length of the re-optimized lens, in millimetres.
    pub efl: f64,
    /// Polychromatic RMS spot radius of the re-optimized lens, in millimetres.
    pub rms: f64,
}

/// Result of a scan.
#[derive(Clone, Debug)]
pub struct Scan {
    /// Effective focal length of the optimized original lens.
    pub baseline_efl: f64,
    /// RMS spot radius of the original lens after the same optimization, in millimetres.
    pub baseline_rms: f64,
    /// Every evaluated swap, best (smallest spot) first.
    pub swaps: Vec<Swap>,
    /// Swaps that could not be evaluated, for example because the new lens has no usable focus.
    pub skipped: usize,
}

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

/// Optimizes `lens` and returns `(effective focal length, RMS spot radius)`.
fn evaluate(
    lens: &Lens,
    target_efl: f64,
    pupil_radius: f64,
    iterations: usize,
) -> Option<(f64, f64)> {
    let focus = lens.paraxial(D_LINE_NM)?;
    if !(focus.bfd.is_finite() && focus.bfd > 0.0) {
        return None;
    }
    let vary: Vec<usize> = (0..lens.surfaces().len()).collect();
    let mut problem = Problem::new(lens.with_image_distance(focus.bfd), vary, target_efl);
    problem.pupil_radius = pupil_radius;
    let outcome = optimize(&problem, iterations).ok()?;
    let efl = outcome.lens.paraxial(D_LINE_NM)?.efl;
    let rms = rms_spot_radius(&outcome.lens, &problem);
    if efl.is_finite() && rms.is_finite() {
        Some((efl, rms))
    } else {
        None
    }
}

/// Scans every glass swap of `lens`. `pupil_radius` is the entrance pupil radius used to judge the
/// spot size, and `iterations` (1 to `MAX_ITERATIONS`) the optimizer budget for each candidate.
pub fn scan(lens: &Lens, pupil_radius: f64, iterations: usize) -> Result<Scan> {
    if !(pupil_radius.is_finite() && pupil_radius > 0.0) {
        return Err(invalid("pupil_radius", "must be a positive number"));
    }
    if !(1..=MAX_ITERATIONS).contains(&iterations) {
        return Err(invalid(
            "iterations",
            &format!("must be between 1 and {MAX_ITERATIONS}"),
        ));
    }
    let start = lens
        .paraxial(D_LINE_NM)
        .ok_or_else(|| invalid("lens", "has no finite focal length"))?;
    let (baseline_efl, baseline_rms) = evaluate(lens, start.efl, pupil_radius, iterations)
        .ok_or_else(|| invalid("lens", "cannot be optimized"))?;

    let mut swaps = Vec::new();
    let mut skipped = 0;
    for (index, surface) in lens.surfaces().iter().enumerate() {
        let Some(original) = surface.glass else {
            continue;
        };
        for &glass in catalog() {
            if glass.name == original.name || NOT_USABLE.contains(&glass.name) {
                continue;
            }
            let mut surfaces = lens.surfaces().to_vec();
            surfaces[index].glass = Some(glass);
            let evaluated = Lens::new(surfaces)
                .ok()
                .and_then(|candidate| evaluate(&candidate, start.efl, pupil_radius, iterations));
            if let Some((efl, rms)) = evaluated {
                swaps.push(Swap {
                    surface: index,
                    from: original.name,
                    to: glass.name,
                    efl,
                    rms,
                });
            } else {
                skipped += 1;
            }
        }
    }
    swaps.sort_by(|a, b| a.rms.total_cmp(&b.rms));
    Ok(Scan {
        baseline_efl,
        baseline_rms,
        swaps,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ITERATIONS: usize = 10;

    fn doublet() -> Lens {
        Lens::parse(include_str!("../../../examples/doublet.lens")).expect("valid lens")
    }

    fn plano() -> Lens {
        Lens::parse(include_str!("../../../examples/plano-convex.lens")).expect("valid lens")
    }

    fn rejected_name(result: &Result<Scan>) -> &'static str {
        match result {
            Err(PrismError::InvalidParameter { name, .. }) => name,
            _ => "",
        }
    }

    #[test]
    fn scans_every_glass_slot_of_a_doublet() {
        let lens = doublet();
        let result = scan(&lens, 5.0, TEST_ITERATIONS).expect("scans");
        assert_eq!(result.swaps.len() + result.skipped, 18);
        assert_ne!(result.swaps.len(), 0);
        for pair in result.swaps.windows(2) {
            assert!(pair[0].rms <= pair[1].rms, "not sorted");
        }
        for swap in &result.swaps {
            assert!(swap.surface < 2, "surface {}", swap.surface);
            let original = if swap.surface == 0 { "N-BK7" } else { "F2" };
            assert_eq!(swap.from, original);
            assert_ne!(swap.to, original);
            assert_ne!(swap.to, "WATER");
            assert_ne!(swap.to, "DIAMOND");
            assert!(swap.rms.is_finite() && swap.rms >= 0.0, "{swap:?}");
            assert!(swap.efl.is_finite(), "{swap:?}");
        }
    }

    #[test]
    fn the_baseline_keeps_the_focal_length() {
        let lens = doublet();
        let original = lens.paraxial(D_LINE_NM).expect("finite focus").efl;
        let result = scan(&lens, 5.0, TEST_ITERATIONS).expect("scans");
        assert!(
            (result.baseline_efl - original).abs() < 1.0,
            "{} versus {original}",
            result.baseline_efl
        );
        assert!(result.baseline_rms.is_finite() && result.baseline_rms >= 0.0);
    }

    #[test]
    fn a_singlet_has_one_glass_slot_and_the_scan_is_deterministic() {
        let lens = plano();
        let first = scan(&lens, 5.0, TEST_ITERATIONS).expect("scans");
        let second = scan(&lens, 5.0, TEST_ITERATIONS).expect("scans");
        assert_eq!(first.swaps.len() + first.skipped, 9);
        assert!(first
            .swaps
            .iter()
            .all(|s| s.surface == 0 && s.from == "N-BK7"));
        assert_eq!(first.swaps, second.swaps);
        assert_eq!(first.skipped, second.skipped);
    }

    #[test]
    fn bad_requests_are_rejected() {
        let lens = plano();
        assert_eq!(rejected_name(&scan(&lens, 0.0, 10)), "pupil_radius");
        assert_eq!(rejected_name(&scan(&lens, f64::NAN, 10)), "pupil_radius");
        assert_eq!(rejected_name(&scan(&lens, 5.0, 0)), "iterations");
        assert_eq!(
            rejected_name(&scan(&lens, 5.0, MAX_ITERATIONS + 1)),
            "iterations"
        );
        let plate = Lens::parse("flat 5 N-BK7 12\nflat 10 air 12\n").expect("valid lens");
        assert_eq!(rejected_name(&scan(&plate, 5.0, 10)), "lens");
    }
}
