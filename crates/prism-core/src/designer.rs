//! Achromatic doublet designer.
//!
//! A pair of glasses with Abbe numbers `V1 > V2` is achromatic (the F and C foci coincide) when
//! the crown carries the fraction `V1 / (V1 - V2)` of the total power and the flint the fraction
//! `-V2 / (V1 - V2)`. What is left over is the secondary spectrum: the g line focuses at
//! `f * (P2 - P1) / (V1 - V2)` away from the F and C foci, where `P` is the relative partial
//! dispersion. Ordinary crown and flint pairs all give about the same value, but pairs that use a
//! glass far off the usual `P` versus `V` line, such as fluorite, give much less. This module ranks
//! every pair in the catalogue by that number and builds a cemented doublet from any pair.

use crate::error::{PrismError, Result};
use crate::glass::{catalog, Glass};
use crate::lens::{Lens, Surface, D_LINE_NM};

const MIN_ABBE_GAP: f64 = 10.0;
const MAX_SCALE_ITERATIONS: usize = 40;
const SCALE_TOLERANCE: f64 = 1e-10;
const FINAL_TOLERANCE: f64 = 1e-6;
/// Materials that cannot be ground and cemented into a doublet.
const NOT_DESIGNABLE: [&str; 2] = ["WATER", "DIAMOND"];

/// A crown and flint pair together with its achromat figures of merit.
#[derive(Clone, Copy, Debug)]
pub struct PairScore {
    /// The glass with the higher Abbe number.
    pub crown: &'static Glass,
    /// The glass with the lower Abbe number.
    pub flint: &'static Glass,
    /// Signed secondary spectrum `(P1 - P2) / (V1 - V2)`; the g-F focal shift is `-secondary * f`.
    pub secondary: f64,
    /// Crown power as a fraction of the total power.
    pub crown_power: f64,
    /// Flint power as a fraction of the total power (negative).
    pub flint_power: f64,
}

impl PairScore {
    /// Largest absolute element power as a multiple of the total power. Large values mean
    /// strongly curved surfaces that are hard to build and make spherical aberration worse.
    pub fn power_ratio(&self) -> f64 {
        self.crown_power.abs().max(self.flint_power.abs())
    }

    /// Focal length of the g line minus that of the F and C lines, for a doublet of focal length `efl`.
    pub fn focal_shift(&self, efl: f64) -> f64 {
        -self.secondary * efl
    }
}

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

fn positive(name: &'static str, value: f64) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(invalid(name, "must be a positive number"))
    }
}

fn designable(glass: &Glass) -> bool {
    !NOT_DESIGNABLE.contains(&glass.name)
}

/// Scores one crown and flint pair; `None` when the two Abbe numbers are equal.
pub fn score_pair(crown: &'static Glass, flint: &'static Glass) -> Option<PairScore> {
    let (v1, v2) = (crown.abbe(), flint.abbe());
    let gap = v1 - v2;
    if !(gap.is_finite() && gap.abs() > 1e-9) {
        return None;
    }
    Some(PairScore {
        crown,
        flint,
        secondary: (crown.partial_dispersion() - flint.partial_dispersion()) / gap,
        crown_power: v1 / gap,
        flint_power: -v2 / gap,
    })
}

/// Every designable pair whose Abbe numbers differ by at least 10 and whose strongest element
/// carries at most `max_power_ratio` times the total power, best (smallest secondary spectrum) first.
pub fn rank_pairs(max_power_ratio: f64) -> Vec<PairScore> {
    let mut pairs = Vec::new();
    for &crown in catalog() {
        for &flint in catalog() {
            if !designable(crown) || !designable(flint) {
                continue;
            }
            if crown.abbe() - flint.abbe() < MIN_ABBE_GAP {
                continue;
            }
            if let Some(score) =
                score_pair(crown, flint).filter(|s| s.power_ratio() <= max_power_ratio)
            {
                pairs.push(score);
            }
        }
    }
    pairs.sort_by(|a, b| a.secondary.abs().total_cmp(&b.secondary.abs()));
    pairs
}

/// Builds a cemented doublet (crown in front) with the requested effective focal length at the
/// d line. The image plane is placed at the paraxial focus. Thicknesses and the semi-aperture are
/// in millimetres.
pub fn design_doublet(
    crown: &'static Glass,
    flint: &'static Glass,
    efl: f64,
    semi_aperture: f64,
    crown_thickness: f64,
    flint_thickness: f64,
) -> Result<Lens> {
    positive("efl", efl)?;
    positive("semi_aperture", semi_aperture)?;
    positive("thickness", crown_thickness)?;
    positive("thickness", flint_thickness)?;
    let pair = score_pair(crown, flint)
        .ok_or_else(|| invalid("glass", "the two glasses have the same Abbe number"))?;
    if pair.crown.abbe() <= pair.flint.abbe() {
        return Err(invalid(
            "glass",
            "the crown must have the higher Abbe number",
        ));
    }

    let power = 1.0 / efl;
    let n1 = crown.index(D_LINE_NM);
    let n2 = flint.index(D_LINE_NM);
    let phi1 = power * pair.crown_power;
    let phi2 = power * pair.flint_power;
    let r1 = 2.0 * (n1 - 1.0) / phi1;
    let r2 = -r1;
    let inverse_r3 = 1.0 / r2 - phi2 / (n2 - 1.0);
    let r3 = if inverse_r3.abs() < 1e-12 {
        0.0
    } else {
        1.0 / inverse_r3
    };

    let build = |scale: f64| {
        Lens::new(vec![
            Surface {
                radius: r1 * scale,
                thickness: crown_thickness,
                glass: Some(crown),
                semi_aperture,
            },
            Surface {
                radius: r2 * scale,
                thickness: flint_thickness,
                glass: Some(flint),
                semi_aperture,
            },
            Surface {
                radius: r3 * scale,
                thickness: 1.0,
                glass: None,
                semi_aperture,
            },
        ])
    };
    let no_focus = || invalid("efl", "the design has no finite focal length");

    // The thin-lens radii give a focal length that is slightly off for a thick lens, so scale
    // every radius until the paraxial focal length matches.
    let mut scale = 1.0;
    for _ in 0..MAX_SCALE_ITERATIONS {
        let current = build(scale)?.paraxial(D_LINE_NM).ok_or_else(no_focus)?;
        let ratio = efl / current.efl;
        scale *= ratio;
        if (ratio - 1.0).abs() < SCALE_TOLERANCE {
            break;
        }
    }
    let lens = build(scale)?;
    let focus = lens.paraxial(D_LINE_NM).ok_or_else(no_focus)?;
    if (focus.efl - efl).abs() > FINAL_TOLERANCE * efl {
        return Err(invalid("efl", "could not reach the requested focal length"));
    }
    if focus.bfd.is_nan() || focus.bfd <= 0.0 {
        return Err(invalid("efl", "the focus falls inside the lens"));
    }
    if lens
        .surfaces()
        .iter()
        .any(|s| s.radius != 0.0 && s.radius.abs() < semi_aperture * 1.02)
    {
        return Err(invalid(
            "semi_aperture",
            "too large for the surface curvatures of this design",
        ));
    }
    Ok(lens.with_image_distance(focus.bfd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glass::{BK7, F2, WATER};

    const G_NM: f64 = 435.83;
    const F_NM: f64 = 486.13;

    fn find(ranked: &[PairScore], crown: &str, flint: &str) -> Option<PairScore> {
        ranked
            .iter()
            .copied()
            .find(|p| p.crown.name == crown && p.flint.name == flint)
    }

    #[test]
    fn ranking_is_sorted_and_respects_the_limits() {
        let ranked = rank_pairs(4.0);
        assert!(ranked.len() > 5, "only {} pairs", ranked.len());
        for pair in &ranked {
            assert!(pair.crown.abbe() - pair.flint.abbe() >= MIN_ABBE_GAP - 1e-9);
            assert!(pair.power_ratio() <= 4.0 + 1e-12);
            assert!(designable(pair.crown) && designable(pair.flint));
        }
        for w in ranked.windows(2) {
            assert!(w[0].secondary.abs() <= w[1].secondary.abs());
        }
    }

    #[test]
    fn a_tighter_power_limit_keeps_fewer_pairs() {
        assert!(rank_pairs(2.0).len() < rank_pairs(4.0).len());
        assert_eq!(rank_pairs(0.5).len(), 0);
    }

    #[test]
    fn power_split_sums_to_one() {
        for pair in rank_pairs(4.0) {
            assert!((pair.crown_power + pair.flint_power - 1.0).abs() < 1e-12);
            assert!(pair.crown_power > 1.0 && pair.flint_power < 0.0);
        }
    }

    #[test]
    fn best_pair_beats_the_classic_bk7_f2_doublet() {
        let ranked = rank_pairs(4.0);
        let baseline = find(&ranked, "N-BK7", "F2").expect("the classic pair is ranked");
        let best = ranked.first().expect("pairs exist");
        assert!(
            best.secondary.abs() < 0.8 * baseline.secondary.abs(),
            "best {} versus baseline {}",
            best.secondary,
            baseline.secondary
        );
    }

    #[test]
    fn liquids_and_diamond_are_not_designable() {
        let ranked = rank_pairs(4.0);
        assert!(ranked
            .iter()
            .all(|p| p.crown.name != "WATER" && p.flint.name != "WATER"));
        assert!(ranked
            .iter()
            .all(|p| p.crown.name != "DIAMOND" && p.flint.name != "DIAMOND"));
        assert!(!designable(&WATER));
    }

    #[test]
    fn focal_shift_scales_with_the_focal_length() {
        let pair = score_pair(&BK7, &F2).expect("scores");
        assert!((pair.focal_shift(200.0) - 2.0 * pair.focal_shift(100.0)).abs() < 1e-12);
    }

    #[test]
    fn equal_glasses_cannot_be_scored() {
        assert!(score_pair(&BK7, &BK7).is_none());
    }

    #[test]
    fn secondary_spectrum_prediction_matches_a_paraxial_trace() {
        let lens = design_doublet(&BK7, &F2, 100.0, 12.5, 0.01, 0.01).expect("designs");
        let pair = score_pair(&BK7, &F2).expect("scores");
        let efl_g = lens.paraxial(G_NM).expect("finite focus").efl;
        let efl_f = lens.paraxial(F_NM).expect("finite focus").efl;
        let measured = (efl_g - efl_f) / 100.0;
        let predicted = -pair.secondary;
        assert!(predicted > 0.0);
        assert!(
            (measured - predicted).abs() < 0.05 * predicted,
            "measured {measured}, predicted {predicted}"
        );
    }

    #[test]
    fn designed_doublets_hit_the_requested_focal_length() {
        let lens = design_doublet(&BK7, &F2, 100.0, 12.5, 4.0, 2.5).expect("designs");
        let focus = lens.paraxial(D_LINE_NM).expect("finite focus");
        assert!((focus.efl - 100.0).abs() < 1e-4, "efl {}", focus.efl);
        assert_eq!(lens.surfaces().len(), 3);
        let last = lens.surfaces()[2];
        assert!((last.thickness - focus.bfd).abs() < 1e-9);
    }

    #[test]
    fn the_best_pair_beats_a_singlet_on_chromatic_focus() {
        let ranked = rank_pairs(4.0);
        let best = ranked.first().expect("pairs exist");
        let doublet =
            design_doublet(best.crown, best.flint, 100.0, 12.5, 4.0, 2.5).expect("designs");
        let focus = doublet.paraxial(D_LINE_NM).expect("finite focus");
        assert!((focus.efl - 100.0).abs() < 1e-4);
        let singlet = Lens::parse("51.68 5.0 N-BK7 12.5\nflat 90 air 12.5\n").expect("valid lens");
        let shift = |lens: &Lens| {
            let blue = lens.paraxial(450.0).expect("finite focus").bfd;
            let red = lens.paraxial(650.0).expect("finite focus").bfd;
            (blue - red).abs()
        };
        assert!(
            shift(&doublet) < 0.35 * shift(&singlet),
            "doublet {}, singlet {}",
            shift(&doublet),
            shift(&singlet)
        );
    }

    #[test]
    fn rejects_unusable_requests() {
        let bad = |efl: f64, aperture: f64, t1: f64, t2: f64| {
            design_doublet(&BK7, &F2, efl, aperture, t1, t2).err()
        };
        assert!(matches!(
            bad(0.0, 12.5, 4.0, 2.5),
            Some(PrismError::InvalidParameter { name: "efl", .. })
        ));
        assert!(matches!(
            bad(f64::NAN, 12.5, 4.0, 2.5),
            Some(PrismError::InvalidParameter { name: "efl", .. })
        ));
        assert!(matches!(
            bad(100.0, 0.0, 4.0, 2.5),
            Some(PrismError::InvalidParameter {
                name: "semi_aperture",
                ..
            })
        ));
        assert!(matches!(
            bad(100.0, 12.5, 0.0, 2.5),
            Some(PrismError::InvalidParameter {
                name: "thickness",
                ..
            })
        ));
        assert!(matches!(
            bad(100.0, 60.0, 4.0, 2.5),
            Some(PrismError::InvalidParameter {
                name: "semi_aperture",
                ..
            })
        ));
        assert!(matches!(
            design_doublet(&BK7, &BK7, 100.0, 12.5, 4.0, 2.5).err(),
            Some(PrismError::InvalidParameter { name: "glass", .. })
        ));
        assert!(matches!(
            design_doublet(&F2, &BK7, 100.0, 12.5, 4.0, 2.5).err(),
            Some(PrismError::InvalidParameter { name: "glass", .. })
        ));
    }
}
