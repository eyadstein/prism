//! Monte Carlo tolerance analysis and an inverse tolerance budget.
//!
//! Real lenses are never made exactly to the prescription. This module builds many random
//! lenses whose radii and thicknesses are off by a uniform random amount, refocuses each one (the
//! back focal distance is the compensator, as it would be at assembly), measures its polychromatic
//! RMS spot radius, and reports the spread. `loosest_scale` runs the study in reverse: it finds the
//! loosest tolerances for which a chosen fraction of lenses still meets a spot-size limit.
//!
//! Every study reuses the same random draws and only multiplies them by the tolerance scale, so
//! the yield changes smoothly with the scale and the budget search can bisect.

use crate::error::{PrismError, Result};
use crate::lens::{Lens, D_LINE_NM};
use crate::math::Rng;
use crate::optimize::{rms_spot_radius, Problem};

/// Most random lenses one study may build.
pub const MAX_TRIALS: usize = 5000;
/// Largest tolerance multiplier that the budget search will report.
pub const MAX_SCALE: f64 = 32.0;
const BISECTION_STEPS: usize = 14;

/// Manufacturing tolerances; every error is drawn uniformly from plus or minus the tolerance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerances {
    /// Relative tolerance on every curved radius (0.002 means plus or minus 0.2 percent).
    pub radius_fraction: f64,
    /// Tolerance on every thickness except the image distance, in millimetres.
    pub thickness_mm: f64,
}

/// Results of one Monte Carlo study. Lengths are in the units of the lens (millimetres).
#[derive(Clone, Debug)]
pub struct Study {
    /// RMS spot radius of the nominal design, refocused.
    pub nominal: f64,
    /// RMS spot radius of every random lens that could be evaluated, smallest first.
    pub samples: Vec<f64>,
    /// Random lenses that could not be evaluated (no usable focus); they count as failures.
    pub failures: usize,
}

impl Study {
    /// Number of random lenses built.
    pub fn trials(&self) -> usize {
        self.samples.len() + self.failures
    }

    /// Spot radius that the fraction `p` of all lenses stays within. Failed lenses rank last,
    /// so the result is infinite when `p` reaches into them.
    pub fn percentile(&self, p: f64) -> f64 {
        let total = self.trials();
        if total == 0 {
            return f64::NAN;
        }
        let p = p.clamp(0.0, 1.0);
        let rank = ((p * total as f64).ceil() as usize).max(1);
        self.samples.get(rank - 1).copied().unwrap_or(f64::INFINITY)
    }

    /// Fraction of all lenses whose spot radius is at most `limit`.
    pub fn yield_fraction(&self, limit: f64) -> f64 {
        let total = self.trials();
        if total == 0 {
            return 0.0;
        }
        self.samples.partition_point(|v| *v <= limit) as f64 / total as f64
    }
}

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

fn validate(tol: &Tolerances, scale: f64, trials: usize) -> Result<()> {
    if !(1..=MAX_TRIALS).contains(&trials) {
        return Err(invalid(
            "trials",
            &format!("must be between 1 and {MAX_TRIALS}"),
        ));
    }
    if !(tol.radius_fraction.is_finite() && tol.radius_fraction >= 0.0) {
        return Err(invalid("radius_fraction", "must not be negative"));
    }
    if !(tol.thickness_mm.is_finite() && tol.thickness_mm >= 0.0) {
        return Err(invalid("thickness_mm", "must not be negative"));
    }
    if !(scale.is_finite() && scale >= 0.0) {
        return Err(invalid("scale", "must not be negative"));
    }
    Ok(())
}

/// The lens with its image plane moved to the d-line paraxial focus; `None` without a real focus.
fn compensate(lens: &Lens) -> Option<Lens> {
    let focus = lens.paraxial(D_LINE_NM)?;
    (focus.bfd.is_finite() && focus.bfd > 0.0).then(|| lens.with_image_distance(focus.bfd))
}

/// A copy of `lens` with every curved radius and every thickness (except the last) perturbed.
/// Exactly two random numbers are drawn per surface whatever `scale` is.
fn perturb(lens: &Lens, tol: &Tolerances, scale: f64, rng: &mut Rng) -> Option<Lens> {
    let count = lens.surfaces().len();
    let mut surfaces = lens.surfaces().to_vec();
    for (i, s) in surfaces.iter_mut().enumerate() {
        let radius_draw = rng.range(-1.0, 1.0);
        let thickness_draw = rng.range(-1.0, 1.0);
        if s.radius != 0.0 {
            s.radius *= 1.0 + scale * tol.radius_fraction * radius_draw;
        }
        if i + 1 < count {
            s.thickness = (s.thickness + scale * tol.thickness_mm * thickness_draw).max(0.0);
        }
    }
    Lens::new(surfaces).ok()
}

/// Builds `trials` random lenses with the tolerances multiplied by `scale`, and measures the RMS
/// spot radius of each with the pupil, fields and wavelengths of `problem`.
pub fn monte_carlo(
    problem: &Problem,
    tol: &Tolerances,
    scale: f64,
    trials: usize,
    seed: u64,
) -> Result<Study> {
    validate(tol, scale, trials)?;
    let nominal_lens = compensate(&problem.lens)
        .ok_or_else(|| invalid("lens", "the nominal lens has no usable focus"))?;
    let nominal = rms_spot_radius(&nominal_lens, problem);
    let mut rng = Rng::new(seed);
    let mut samples = Vec::with_capacity(trials);
    let mut failures = 0;
    for _ in 0..trials {
        let value = perturb(&problem.lens, tol, scale, &mut rng)
            .and_then(|lens| compensate(&lens))
            .map(|lens| rms_spot_radius(&lens, problem))
            .filter(|v| v.is_finite());
        if let Some(v) = value {
            samples.push(v);
        } else {
            failures += 1;
        }
    }
    samples.sort_by(f64::total_cmp);
    Ok(Study {
        nominal,
        samples,
        failures,
    })
}

/// The largest multiplier of `tol` for which at least `target_yield` of the random lenses have an
/// RMS spot radius of at most `spec` (in lens units). Returns `Ok(None)` when even the nominal
/// design misses the limit, and `Some(MAX_SCALE)` when the search limit is reached.
pub fn loosest_scale(
    problem: &Problem,
    tol: &Tolerances,
    spec: f64,
    target_yield: f64,
    trials: usize,
    seed: u64,
) -> Result<Option<f64>> {
    if !(spec.is_finite() && spec > 0.0) {
        return Err(invalid("spec", "must be a positive number"));
    }
    if !(target_yield > 0.0 && target_yield <= 1.0) {
        return Err(invalid("target_yield", "must be above 0 and at most 1"));
    }
    let passes = |scale: f64| -> Result<bool> {
        let study = monte_carlo(problem, tol, scale, trials, seed)?;
        Ok(study.yield_fraction(spec) >= target_yield)
    };
    if !passes(0.0)? {
        return Ok(None);
    }
    if passes(MAX_SCALE)? {
        return Ok(Some(MAX_SCALE));
    }
    let (mut low, mut high) = (0.0, MAX_SCALE);
    for _ in 0..BISECTION_STEPS {
        let mid = f64::midpoint(low, high);
        if passes(mid)? {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(Some(low))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doublet() -> Lens {
        Lens::parse(include_str!("../../../examples/doublet.lens")).expect("valid lens")
    }

    fn problem() -> Problem {
        let lens = doublet();
        let efl = lens.paraxial(D_LINE_NM).expect("finite focus").efl;
        Problem::new(lens, Vec::new(), efl)
    }

    fn tol() -> Tolerances {
        Tolerances {
            radius_fraction: 0.002,
            thickness_mm: 0.05,
        }
    }

    fn spread(study: &Study) -> f64 {
        let first = study.samples.first().copied().expect("samples exist");
        let last = study.samples.last().copied().expect("samples exist");
        last - first
    }

    fn rejected(result: &Result<Study>, expected: &str) -> bool {
        matches!(
            result,
            Err(PrismError::InvalidParameter { name, .. }) if *name == expected
        )
    }
    #[test]
    fn zero_scale_reproduces_the_nominal_design() {
        let study = monte_carlo(&problem(), &tol(), 0.0, 20, 1).expect("runs");
        assert_eq!(study.samples.len(), 20);
        assert_eq!(study.failures, 0);
        assert!(study.nominal > 0.0);
        for v in &study.samples {
            assert!(
                (v - study.nominal).abs() < 1e-12,
                "{v} versus {}",
                study.nominal
            );
        }
    }

    #[test]
    fn larger_tolerances_spread_the_results() {
        let small = monte_carlo(&problem(), &tol(), 1.0, 60, 7).expect("runs");
        let large = monte_carlo(&problem(), &tol(), 3.0, 60, 7).expect("runs");
        assert!(spread(&small) > 0.0);
        assert!(
            spread(&large) > spread(&small),
            "{} then {}",
            spread(&small),
            spread(&large)
        );
    }

    #[test]
    fn results_are_reproducible_and_depend_on_the_seed() {
        let a = monte_carlo(&problem(), &tol(), 1.0, 30, 5).expect("runs");
        let b = monte_carlo(&problem(), &tol(), 1.0, 30, 5).expect("runs");
        let c = monte_carlo(&problem(), &tol(), 1.0, 30, 6).expect("runs");
        assert_eq!(a.samples, b.samples);
        assert_ne!(a.samples, c.samples);
    }

    #[test]
    fn percentiles_and_yield_are_consistent() {
        let study = monte_carlo(&problem(), &tol(), 2.0, 50, 3).expect("runs");
        let (p0, p50, p90, p100) = (
            study.percentile(0.0),
            study.percentile(0.5),
            study.percentile(0.9),
            study.percentile(1.0),
        );
        assert!(p0 <= p50 && p50 <= p90 && p90 <= p100);
        assert!((p0 - study.samples[0]).abs() < 1e-15);
        assert!((p100 - study.samples[study.samples.len() - 1]).abs() < 1e-15);
        assert!((study.yield_fraction(f64::INFINITY) - 1.0).abs() < 1e-12);
        assert!(study.yield_fraction(0.0).abs() < 1e-12);
        assert!(study.yield_fraction(p50) >= 0.5);
    }

    #[test]
    fn failures_count_against_the_yield() {
        let study = Study {
            nominal: 1.0,
            samples: vec![1.0, 2.0, 3.0],
            failures: 1,
        };
        assert_eq!(study.trials(), 4);
        assert!((study.yield_fraction(10.0) - 0.75).abs() < 1e-12);
        assert!((study.yield_fraction(2.0) - 0.5).abs() < 1e-12);
        assert!((study.percentile(0.75) - 3.0).abs() < 1e-12);
        assert!(study.percentile(0.76).is_infinite());
        assert!(study.percentile(1.0).is_infinite());
    }

    #[test]
    fn empty_studies_are_safe() {
        let study = Study {
            nominal: 1.0,
            samples: Vec::new(),
            failures: 0,
        };
        assert!(study.percentile(0.5).is_nan());
        assert!(study.yield_fraction(1.0).abs() < 1e-12);
    }

    #[test]
    fn budget_meets_the_requested_yield() {
        let p = problem();
        let nominal = monte_carlo(&p, &tol(), 0.0, 1, 1).expect("runs").nominal;
        let spec = 3.0 * nominal;
        let scale = loosest_scale(&p, &tol(), spec, 0.9, 60, 1)
            .expect("runs")
            .expect("the nominal design meets the limit");
        assert!(scale > 0.0 && scale < MAX_SCALE, "scale {scale}");
        let at_budget = monte_carlo(&p, &tol(), scale, 60, 1).expect("runs");
        assert!(at_budget.yield_fraction(spec) >= 0.9);
    }

    #[test]
    fn impossible_specs_have_no_budget() {
        let p = problem();
        let nominal = monte_carlo(&p, &tol(), 0.0, 1, 1).expect("runs").nominal;
        let result = loosest_scale(&p, &tol(), 0.5 * nominal, 0.9, 20, 1).expect("runs");
        assert_eq!(result, None);
    }

    #[test]
    fn generous_specs_hit_the_search_limit() {
        let result = loosest_scale(&problem(), &tol(), 1000.0, 0.9, 20, 1).expect("runs");
        assert_eq!(result, Some(MAX_SCALE));
    }

    #[test]
    fn afocal_lenses_cannot_be_studied() {
        let plate = Lens::parse("flat 5 N-BK7 12\nflat 10 air 12\n").expect("valid lens");
        let p = Problem::new(plate, Vec::new(), 100.0);
        assert!(rejected(&monte_carlo(&p, &tol(), 1.0, 5, 1), "lens"));
    }

    #[test]
    fn bad_inputs_are_rejected() {
        let p = problem();
        assert!(rejected(&monte_carlo(&p, &tol(), 1.0, 0, 1), "trials"));
        assert!(rejected(
            &monte_carlo(&p, &tol(), 1.0, MAX_TRIALS + 1, 1),
            "trials"
        ));
        assert!(rejected(&monte_carlo(&p, &tol(), f64::NAN, 5, 1), "scale"));
        assert!(rejected(&monte_carlo(&p, &tol(), -1.0, 5, 1), "scale"));
        let negative_radius = Tolerances {
            radius_fraction: -0.1,
            ..tol()
        };
        assert!(rejected(
            &monte_carlo(&p, &negative_radius, 1.0, 5, 1),
            "radius_fraction"
        ));
        let negative_thickness = Tolerances {
            thickness_mm: f64::NAN,
            ..tol()
        };
        assert!(rejected(
            &monte_carlo(&p, &negative_thickness, 1.0, 5, 1),
            "thickness_mm"
        ));
        assert!(matches!(
            loosest_scale(&p, &tol(), 0.0, 0.9, 5, 1),
            Err(PrismError::InvalidParameter { name: "spec", .. })
        ));
        assert!(matches!(
            loosest_scale(&p, &tol(), 1.0, 0.0, 5, 1),
            Err(PrismError::InvalidParameter {
                name: "target_yield",
                ..
            })
        ));
        assert!(matches!(
            loosest_scale(&p, &tol(), 1.0, 1.5, 5, 1),
            Err(PrismError::InvalidParameter {
                name: "target_yield",
                ..
            })
        ));
    }
}
