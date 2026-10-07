//! Lens optimization with damped least squares (Levenberg-Marquardt).
//!
//! The variables are the curvatures of the chosen surfaces plus the image distance. The
//! residuals are the image-plane offsets of a grid of rays at several wavelengths and field
//! angles, measured from the d-line chief ray, plus a weighted effective focal length error.

use crate::check_wavelength;
use crate::error::{PrismError, Result};
use crate::lens::{Lens, D_LINE_NM};
use crate::math::{Ray, Vec3};

const CURVATURE_STEP: f64 = 1e-7;
const DISTANCE_STEP: f64 = 1e-5;
const DAMPING_TRIES: usize = 12;

/// What to optimize and how to judge a design.
#[derive(Clone, Debug)]
pub struct Problem {
    /// Starting design.
    pub lens: Lens,
    /// Indices of the surfaces whose curvature may change. The image distance always varies.
    pub variable_surfaces: Vec<usize>,
    /// Required effective focal length at the d line.
    pub target_efl: f64,
    /// Entrance pupil radius.
    pub pupil_radius: f64,
    /// Field angles in degrees.
    pub fields_deg: Vec<f64>,
    /// Wavelengths in nanometres.
    pub wavelengths_nm: Vec<f64>,
    /// Rays per side of the pupil grid.
    pub grid: usize,
    /// Weight of the focal length error relative to the spot residuals.
    pub efl_weight: f64,
}

impl Problem {
    /// A problem with sensible defaults: pupil radius 5, on-axis field, 450/550/650 nm.
    pub fn new(lens: Lens, variable_surfaces: Vec<usize>, target_efl: f64) -> Self {
        Self {
            lens,
            variable_surfaces,
            target_efl,
            pupil_radius: 5.0,
            fields_deg: vec![0.0],
            wavelengths_nm: vec![450.0, 550.0, 650.0],
            grid: 9,
            efl_weight: 10.0,
        }
    }
}

/// Result of an optimization run.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The optimized design.
    pub lens: Lens,
    /// Sum of squared residuals before optimizing.
    pub initial_cost: f64,
    /// Sum of squared residuals after optimizing.
    pub final_cost: f64,
    /// Iterations performed.
    pub iterations: usize,
}

fn invalid(name: &'static str, reason: &str) -> PrismError {
    PrismError::InvalidParameter {
        name,
        reason: reason.to_owned(),
    }
}

fn validate(p: &Problem) -> Result<()> {
    if !(p.target_efl.is_finite() && p.target_efl > 0.0) {
        return Err(invalid("target_efl", "must be a positive number"));
    }
    if !(p.pupil_radius.is_finite() && p.pupil_radius > 0.0) {
        return Err(invalid("pupil_radius", "must be a positive number"));
    }
    if p.grid == 0 {
        return Err(invalid("grid", "must be at least 1"));
    }
    if !(p.efl_weight.is_finite() && p.efl_weight >= 0.0) {
        return Err(invalid("efl_weight", "must not be negative"));
    }
    if p.fields_deg.is_empty() || p.fields_deg.iter().any(|f| !f.is_finite()) {
        return Err(invalid(
            "fields_deg",
            "needs at least one finite field angle",
        ));
    }
    if p.wavelengths_nm.is_empty() {
        return Err(invalid("wavelengths_nm", "needs at least one wavelength"));
    }
    for &nm in &p.wavelengths_nm {
        check_wavelength(nm)?;
    }
    let count = p.lens.surfaces().len();
    if let Some(bad) = p.variable_surfaces.iter().find(|&&i| i >= count) {
        return Err(invalid(
            "variable_surfaces",
            &format!("surface index {bad} is out of range"),
        ));
    }
    Ok(())
}

fn sum_sq(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

/// Root-mean-square spot radius implied by a list of `(dx, dy)` residual pairs.
pub fn rms_spot(spot_residuals: &[f64]) -> f64 {
    let pairs = spot_residuals.len() / 2;
    if pairs == 0 {
        return 0.0;
    }
    (sum_sq(spot_residuals) / pairs as f64).sqrt()
}

/// RMS spot radius of `lens` over every wavelength and field of `problem`.
pub fn rms_spot_radius(lens: &Lens, problem: &Problem) -> f64 {
    rms_spot(&spot_residuals(lens, problem))
}

fn spot_residuals(lens: &Lens, p: &Problem) -> Vec<f64> {
    let r = p.pupil_radius;
    let mut out = Vec::new();
    for &field in &p.fields_deg {
        let theta = field.to_radians();
        let dir = Vec3::new(0.0, theta.sin(), theta.cos());
        let reference = lens
            .trace_to_image(Ray::new(Vec3::ZERO, dir), D_LINE_NM)
            .ok();
        for &nm in &p.wavelengths_nm {
            for i in 0..p.grid {
                for j in 0..p.grid {
                    let x = ((i as f64 + 0.5) / p.grid as f64 * 2.0 - 1.0) * r;
                    let y = ((j as f64 + 0.5) / p.grid as f64 * 2.0 - 1.0) * r;
                    if x * x + y * y > r * r {
                        continue;
                    }
                    let hit = lens
                        .trace_to_image(Ray::new(Vec3::new(x, y, 0.0), dir), nm)
                        .ok();
                    if let (Some(h), Some(c)) = (hit, reference) {
                        out.push(h.x - c.x);
                        out.push(h.y - c.y);
                    } else {
                        out.push(r);
                        out.push(r);
                    }
                }
            }
        }
    }
    out
}

fn residuals(lens: &Lens, p: &Problem) -> Option<Vec<f64>> {
    let efl = lens.paraxial(D_LINE_NM)?.efl;
    let mut out = spot_residuals(lens, p);
    out.push(p.efl_weight * (efl - p.target_efl));
    Some(out)
}

fn build(base: &Lens, indices: &[usize], x: &[f64]) -> Option<Lens> {
    let mut surfaces = base.surfaces().to_vec();
    for (k, &i) in indices.iter().enumerate() {
        let c = x[k];
        surfaces[i].radius = if c.abs() < 1e-12 { 0.0 } else { 1.0 / c };
    }
    surfaces.last_mut()?.thickness = x[indices.len()];
    Lens::new(surfaces).ok()
}

fn jacobian(
    eval: &impl Fn(&[f64]) -> Option<Vec<f64>>,
    x: &[f64],
    r: &[f64],
) -> Option<Vec<Vec<f64>>> {
    let n = x.len();
    let mut jac = vec![vec![0.0; n]; r.len()];
    for j in 0..n {
        let h = if j + 1 == n {
            DISTANCE_STEP
        } else {
            CURVATURE_STEP
        };
        let mut shifted = x.to_vec();
        shifted[j] += h;
        let rp = eval(&shifted)?;
        for (row, (a, b)) in rp.iter().zip(r).enumerate() {
            jac[row][j] = (a - b) / h;
        }
    }
    Some(jac)
}

fn normal_equations(jac: &[Vec<f64>], r: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let n = jac.first().map_or(0, Vec::len);
    let mut jtj = vec![vec![0.0; n]; n];
    let mut jtr = vec![0.0; n];
    for (row, res) in jac.iter().zip(r) {
        for a in 0..n {
            jtr[a] += row[a] * res;
            for b in 0..n {
                jtj[a][b] += row[a] * row[b];
            }
        }
    }
    (jtj, jtr)
}

fn damped_step(jtj: &[Vec<f64>], jtr: &[f64], lambda: f64) -> Option<Vec<f64>> {
    let mut a = jtj.to_vec();
    for (k, row) in a.iter_mut().enumerate() {
        row[k] += lambda * jtj[k][k].max(1e-12);
    }
    solve_linear(a, jtr.iter().map(|v| -v).collect())
}

/// Solves `a x = b` by Gaussian elimination with partial pivoting. `a` is row-major, n by n.
/// Returns `None` when the matrix is singular.
pub fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let tail: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    Some(x)
}

/// Runs damped least squares for at most `max_iterations` iterations.
pub fn optimize(problem: &Problem, max_iterations: usize) -> Result<Outcome> {
    validate(problem)?;
    let mut vars = problem.variable_surfaces.clone();
    vars.sort_unstable();
    vars.dedup();
    let base = &problem.lens;
    let eval = |x: &[f64]| build(base, &vars, x).and_then(|lens| residuals(&lens, problem));

    let mut x: Vec<f64> = vars
        .iter()
        .map(|&i| base.surfaces()[i].curvature())
        .collect();
    x.push(base.surfaces().last().map_or(0.0, |s| s.thickness));
    let Some(mut r) = eval(&x) else {
        return Err(invalid("lens", "the starting lens cannot be evaluated"));
    };
    let initial_cost = sum_sq(&r);
    let mut cost = initial_cost;
    let mut lambda = 1e-3;
    let mut iterations = 0;

    while iterations < max_iterations {
        iterations += 1;
        let Some(jac) = jacobian(&eval, &x, &r) else {
            break;
        };
        let (jtj, jtr) = normal_equations(&jac, &r);
        let mut step = None;
        for _ in 0..DAMPING_TRIES {
            let trial = damped_step(&jtj, &jtr, lambda).and_then(|dx| {
                let candidate: Vec<f64> = x.iter().zip(&dx).map(|(a, d)| a + d).collect();
                let residual = eval(&candidate)?;
                let c = sum_sq(&residual);
                (c < cost).then_some((candidate, residual, c))
            });
            if trial.is_some() {
                step = trial;
                break;
            }
            lambda *= 4.0;
        }
        let Some((candidate, residual, c)) = step else {
            break;
        };
        let improvement = cost - c;
        x = candidate;
        r = residual;
        cost = c;
        lambda = (lambda * 0.3).max(1e-12);
        if improvement <= 1e-10 * (cost + improvement) {
            break;
        }
    }

    let lens =
        build(base, &vars, &x).ok_or_else(|| invalid("lens", "the final lens cannot be built"))?;
    Ok(Outcome {
        lens,
        initial_cost,
        final_cost: cost,
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> Lens {
        Lens::parse("60 5 N-BK7 12.5\nflat 90 air 12.5\n").expect("valid lens")
    }

    #[test]
    fn linear_solver_handles_known_systems() {
        let x =
            solve_linear(vec![vec![2.0, 1.0], vec![1.0, 3.0]], vec![4.0, 7.0]).expect("solvable");
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 2.0).abs() < 1e-12);

        let swapped =
            solve_linear(vec![vec![0.0, 1.0], vec![1.0, 0.0]], vec![2.0, 3.0]).expect("solvable");
        assert!((swapped[0] - 3.0).abs() < 1e-12 && (swapped[1] - 2.0).abs() < 1e-12);

        let a = vec![
            vec![3.0, 2.0, -1.0],
            vec![2.0, -2.0, 4.0],
            vec![-1.0, 0.5, -1.0],
        ];
        let y = solve_linear(a, vec![1.0, -2.0, 0.0]).expect("solvable");
        assert!((y[0] - 1.0).abs() < 1e-12);
        assert!((y[1] + 2.0).abs() < 1e-12);
        assert!((y[2] + 2.0).abs() < 1e-12);
    }

    #[test]
    fn singular_systems_are_rejected() {
        assert!(solve_linear(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]).is_none());
    }

    #[test]
    fn rms_spot_of_a_single_pair() {
        assert!((rms_spot(&[3.0, 4.0]) - 5.0).abs() < 1e-12);
        assert!(rms_spot(&[]).abs() < 1e-30);
    }

    #[test]
    fn optimizer_repairs_a_bad_singlet() {
        let mut problem = Problem::new(start(), vec![0], 100.0);
        problem.grid = 7;
        let out = optimize(&problem, 100).expect("optimizes");
        assert!(out.iterations >= 1);
        assert!(
            out.final_cost < 0.1 * out.initial_cost,
            "{} -> {}",
            out.initial_cost,
            out.final_cost
        );
        let efl = out.lens.paraxial(D_LINE_NM).expect("finite focus").efl;
        assert!((efl - 100.0).abs() < 1.0, "efl = {efl}");
    }

    #[test]
    fn multi_surface_optimization_never_gets_worse() {
        let lens = Lens::parse("44.78 4.0 N-BK7 12.5\n-44.78 2.5 F2 12.5\n-812 95.0 air 12.5\n")
            .expect("valid lens");
        let mut problem = Problem::new(lens, vec![0, 1, 2], 100.0);
        problem.grid = 7;
        let out = optimize(&problem, 40).expect("optimizes");
        assert!(out.final_cost <= out.initial_cost);
        assert_eq!(out.lens.surfaces().len(), 3);
    }

    #[test]
    fn bad_problems_are_rejected() {
        let out_of_range = Problem::new(start(), vec![5], 100.0);
        assert!(matches!(
            optimize(&out_of_range, 10),
            Err(PrismError::InvalidParameter {
                name: "variable_surfaces",
                ..
            })
        ));
        let negative = Problem::new(start(), vec![0], -1.0);
        assert!(matches!(
            optimize(&negative, 10),
            Err(PrismError::InvalidParameter {
                name: "target_efl",
                ..
            })
        ));
        let mut ultraviolet = Problem::new(start(), vec![0], 100.0);
        ultraviolet.wavelengths_nm = vec![200.0];
        assert!(matches!(
            optimize(&ultraviolet, 10),
            Err(PrismError::WavelengthOutOfRange(_))
        ));
    }

    #[test]
    fn prescriptions_round_trip() {
        let lens = Lens::parse("# x\n51.68 5.0 N-BK7 12.5\nflat 90.0 air\n").expect("valid");
        let again = Lens::parse(&lens.to_prescription()).expect("round trip");
        assert_eq!(lens.surfaces().len(), again.surfaces().len());
        for (a, b) in lens.surfaces().iter().zip(again.surfaces()) {
            assert!((a.radius - b.radius).abs() < 1e-5);
            assert!((a.thickness - b.thickness).abs() < 1e-5);
            assert_eq!(a.glass.map(|g| g.name), b.glass.map(|g| g.name));
            assert_eq!(a.semi_aperture, b.semi_aperture);
        }
    }
}
