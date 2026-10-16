//! Zernike polynomials in Noll order and least-squares wavefront fitting.
//!
//! The terms are orthonormal over the unit disc, so the root-mean-square value of a wavefront
//! with the piston removed is the square root of the sum of its squared coefficients.

use std::f64::consts::{SQRT_2, TAU};

use crate::error::{PrismError, Result};
use crate::optimize::solve_linear;

/// Most terms a fit may use.
pub const MAX_TERMS: usize = 28;
const EDGE_TOLERANCE: f64 = 1.0e-9;

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

fn factorial(n: u32) -> f64 {
    (1..=n).map(f64::from).product::<f64>()
}

/// Radial order `n`, azimuthal frequency `m` and whether the term is a sine term, for Noll
/// index `j` (counting from 1; 0 is treated as 1). Cosine terms have even `j`.
pub fn noll_indices(j: usize) -> (u32, u32, bool) {
    let j = j.max(1);
    let mut n = 0_usize;
    let mut remaining = j - 1;
    while remaining > n {
        n += 1;
        remaining -= n;
    }
    let m = (n % 2) + 2 * usize::midpoint(remaining, (n + 1) % 2);
    (n as u32, m as u32, m != 0 && (j & 1) == 1)
}

/// Radial polynomial `R(n, m)` evaluated at radius `rho`. It is zero when `m` exceeds `n` or
/// `n - m` is odd.
pub fn radial(n: u32, m: u32, rho: f64) -> f64 {
    if m > n || ((n - m) & 1) == 1 {
        return 0.0;
    }
    let half_difference = (n - m) / 2;
    let half_sum = u32::midpoint(n, m);
    let mut sum = 0.0;
    for k in 0..=half_difference {
        let sign = if (k & 1) == 0 { 1.0 } else { -1.0 };
        let coefficient = factorial(n - k)
            / (factorial(k) * factorial(half_sum - k) * factorial(half_difference - k));
        let power = i32::try_from(n - 2 * k).unwrap_or(0);
        sum += sign * coefficient * rho.powi(power);
    }
    sum
}

/// Orthonormal Zernike term `j` (Noll order) at polar position `(rho, phi)` of the unit disc.
pub fn zernike(j: usize, rho: f64, phi: f64) -> f64 {
    let (n, m, sine) = noll_indices(j);
    let r = radial(n, m, rho);
    let norm = f64::from(n + 1).sqrt();
    if m == 0 {
        return norm * r;
    }
    let angle = f64::from(m) * phi;
    let angular = if sine { angle.sin() } else { angle.cos() };
    norm * SQRT_2 * r * angular
}

/// Common name of Noll term `j`.
pub fn name(j: usize) -> &'static str {
    match j {
        1 => "piston",
        2 => "tilt x",
        3 => "tilt y",
        4 => "defocus",
        5 => "astigmatism 45 deg",
        6 => "astigmatism 0 deg",
        7 => "coma y",
        8 => "coma x",
        9 => "trefoil y",
        10 => "trefoil x",
        11 => "spherical",
        12..=13 => "secondary astigmatism",
        14..=15 => "tetrafoil",
        _ => "higher order",
    }
}

/// Value of the wavefront with the given coefficients (term 1 first) at `(rho, phi)`.
pub fn evaluate(coefficients: &[f64], rho: f64, phi: f64) -> f64 {
    coefficients
        .iter()
        .enumerate()
        .map(|(index, c)| c * zernike(index + 1, rho, phi))
        .sum()
}

/// Least-squares fit of the first `terms` Zernike terms to samples `(x, y, value)` taken at
/// normalized pupil positions inside the unit disc. Returns one coefficient per term.
pub fn fit(points: &[(f64, f64, f64)], terms: usize) -> Result<Vec<f64>> {
    if !(1..=MAX_TERMS).contains(&terms) {
        return Err(invalid(
            "terms",
            &format!("must be between 1 and {MAX_TERMS}"),
        ));
    }
    if points.len() < terms {
        return Err(invalid("points", "there must be at least as many as terms"));
    }
    let unusable = points.iter().any(|&(x, y, w)| {
        !(x.is_finite() && y.is_finite() && w.is_finite()) || x * x + y * y > 1.0 + EDGE_TOLERANCE
    });
    if unusable {
        return Err(invalid("points", "must be finite and inside the unit disc"));
    }
    let mut ata = vec![vec![0.0_f64; terms]; terms];
    let mut atb = vec![0.0_f64; terms];
    for &(x, y, w) in points {
        let rho = x.hypot(y);
        let phi = y.atan2(x);
        let row: Vec<f64> = (1..=terms).map(|j| zernike(j, rho, phi)).collect();
        for a in 0..terms {
            atb[a] += row[a] * w;
            for b in 0..terms {
                ata[a][b] += row[a] * row[b];
            }
        }
    }
    solve_linear(ata, atb)
        .ok_or_else(|| invalid("points", "are too few or too poorly spread to fit"))
}

/// Root-mean-square value of a wavefront from its coefficients. Piston never counts, and with
/// `remove_tilt` the two tilt terms are left out as well.
pub fn rms(coefficients: &[f64], remove_tilt: bool) -> f64 {
    let first = if remove_tilt { 3 } else { 1 };
    coefficients
        .iter()
        .skip(first)
        .map(|c| c * c)
        .sum::<f64>()
        .sqrt()
}

/// Strehl ratio from an RMS wavefront error in waves, by the Marechal approximation
/// `exp(-(2 pi sigma)^2)`. It is accurate for errors below about 0.1 wave.
pub fn strehl_marechal(rms_waves: f64) -> f64 {
    (-(TAU * rms_waves).powi(2)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn disc_points(n: usize) -> Vec<(f64, f64)> {
        let mut points = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let x = -1.0 + 2.0 * j as f64 / (n - 1) as f64;
                let y = -1.0 + 2.0 * i as f64 / (n - 1) as f64;
                if x * x + y * y <= 1.0 {
                    points.push((x, y));
                }
            }
        }
        points
    }

    fn synthetic(points: &[(f64, f64)], terms: &[(usize, f64)]) -> Vec<(f64, f64, f64)> {
        points
            .iter()
            .map(|&(x, y)| {
                let (rho, phi) = (x.hypot(y), y.atan2(x));
                let value: f64 = terms.iter().map(|&(j, c)| c * zernike(j, rho, phi)).sum();
                (x, y, value)
            })
            .collect()
    }

    fn rejected(result: &Result<Vec<f64>>, wanted: &str) -> bool {
        matches!(result, Err(PrismError::InvalidParameter { name, .. }) if *name == wanted)
    }

    #[test]
    fn the_noll_table_matches_the_published_one() {
        let expected: [(usize, (u32, u32, bool)); 15] = [
            (1, (0, 0, false)),
            (2, (1, 1, false)),
            (3, (1, 1, true)),
            (4, (2, 0, false)),
            (5, (2, 2, true)),
            (6, (2, 2, false)),
            (7, (3, 1, true)),
            (8, (3, 1, false)),
            (9, (3, 3, true)),
            (10, (3, 3, false)),
            (11, (4, 0, false)),
            (12, (4, 2, false)),
            (13, (4, 2, true)),
            (14, (4, 4, false)),
            (15, (4, 4, true)),
        ];
        for (j, want) in expected {
            assert_eq!(noll_indices(j), want, "j = {j}");
        }
        assert_eq!(noll_indices(0), noll_indices(1));
    }

    #[test]
    fn radial_polynomials_match_their_closed_forms() {
        assert!((radial(2, 0, 0.7) - (-0.02)).abs() < 1e-12);
        assert!((radial(4, 0, 0.7) - (-0.4994)).abs() < 1e-12);
        assert!((radial(3, 1, 0.5) - (-0.625)).abs() < 1e-12);
        assert!((radial(2, 2, 0.6) - 0.36).abs() < 1e-12);
        for (n, m) in [(0, 0), (1, 1), (2, 0), (3, 1), (4, 0), (4, 4), (5, 3)] {
            assert!((radial(n, m, 1.0) - 1.0).abs() < 1e-12, "R({n}, {m}) at 1");
        }
        assert!(radial(3, 0, 0.5).abs() < 1e-15);
        assert!(radial(2, 3, 0.5).abs() < 1e-15);
    }

    #[test]
    fn known_term_values() {
        assert!((zernike(1, 0.3, 1.1) - 1.0).abs() < 1e-15);
        assert!((zernike(4, 0.0, 0.0) + 3.0_f64.sqrt()).abs() < 1e-12);
        assert!((zernike(11, 0.0, 0.0) - 5.0_f64.sqrt()).abs() < 1e-12);
        assert!((zernike(2, 0.5, 0.0) - 1.0).abs() < 1e-12);
        assert!((zernike(3, 0.5, PI / 2.0) - 1.0).abs() < 1e-12);
        let coefficients = [0.0, 0.0, 0.0, 0.3];
        assert!((evaluate(&coefficients, 0.4, 0.2) - 0.3 * zernike(4, 0.4, 0.2)).abs() < 1e-15);
    }

    #[test]
    fn terms_are_orthonormal_over_the_unit_disc() {
        let terms = 15;
        let (radial_steps, angular_steps) = (200_usize, 64_usize);
        let mut gram = vec![vec![0.0_f64; terms]; terms];
        for i in 0..radial_steps {
            let rho = (i as f64 + 0.5) / radial_steps as f64;
            for l in 0..angular_steps {
                let phi = TAU * l as f64 / angular_steps as f64;
                let row: Vec<f64> = (1..=terms).map(|j| zernike(j, rho, phi)).collect();
                let weight = 2.0 * rho / (radial_steps * angular_steps) as f64;
                for a in 0..terms {
                    for b in 0..terms {
                        gram[a][b] += weight * row[a] * row[b];
                    }
                }
            }
        }
        for a in 0..terms {
            for b in 0..terms {
                let expected = if a == b { 1.0 } else { 0.0 };
                assert!(
                    (gram[a][b] - expected).abs() < 2e-3,
                    "gram[{a}][{b}] = {}",
                    gram[a][b]
                );
            }
        }
    }

    #[test]
    fn a_fit_recovers_known_coefficients() {
        let samples = synthetic(
            &disc_points(21),
            &[(1, 0.05), (4, 0.3), (8, -0.2), (11, 0.1)],
        );
        let c = fit(&samples, 15).expect("fits");
        assert_eq!(c.len(), 15);
        for (index, value) in c.iter().enumerate() {
            let want = match index + 1 {
                1 => 0.05,
                4 => 0.3,
                8 => -0.2,
                11 => 0.1,
                _ => 0.0,
            };
            assert!((value - want).abs() < 1e-8, "term {}: {value}", index + 1);
        }
    }

    #[test]
    fn bad_fits_are_rejected() {
        let samples = synthetic(&disc_points(9), &[(4, 0.1)]);
        assert!(rejected(&fit(&samples, 0), "terms"));
        assert!(rejected(&fit(&samples, MAX_TERMS + 1), "terms"));
        assert!(rejected(&fit(&samples[..5], 10), "points"));
        let mut nan = samples.clone();
        nan[3].2 = f64::NAN;
        assert!(rejected(&fit(&nan, 6), "points"));
        let mut outside = samples.clone();
        outside[0] = (2.0, 0.0, 1.0);
        assert!(rejected(&fit(&outside, 6), "points"));
    }

    #[test]
    fn rms_follows_the_coefficients() {
        let coefficients = [0.5, 0.3, 0.4, 0.1, 0.2];
        assert!((rms(&coefficients, false) - 0.30_f64.sqrt()).abs() < 1e-12);
        assert!((rms(&coefficients, true) - 0.05_f64.sqrt()).abs() < 1e-12);
        assert!(rms(&[], false).abs() < 1e-15);
        assert!(rms(&[1.0], false).abs() < 1e-15);
    }

    #[test]
    fn strehl_follows_marechal() {
        assert!((strehl_marechal(0.0) - 1.0).abs() < 1e-15);
        let expected = (-(2.0 * PI * 0.07).powi(2)).exp();
        assert!((strehl_marechal(0.07) - expected).abs() < 1e-15);
        assert!(strehl_marechal(0.07) > 0.82 && strehl_marechal(0.07) < 0.83);
        assert!(strehl_marechal(0.2) < strehl_marechal(0.1));
    }

    #[test]
    fn terms_have_names() {
        assert_eq!(name(1), "piston");
        assert_eq!(name(4), "defocus");
        assert_eq!(name(11), "spherical");
        assert_eq!(name(13), "secondary astigmatism");
        assert_eq!(name(99), "higher order");
    }
}
