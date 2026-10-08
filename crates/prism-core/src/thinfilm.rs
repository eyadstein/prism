//! Thin-film interference: the reflectance of an infinitely thin transparent film in air.
//!
//! Multiple-beam (Airy) interference with air on both sides of the film gives
//! `R = F sin^2(b) / (1 + F sin^2(b))`, where `b = 2 pi n d cos(t) / lambda` is the one-way
//! phase thickness, `F = 4 r^2 / (1 - r^2)^2`, and `r` is the Fresnel amplitude coefficient of
//! one air/film interface for the polarization in question. Light that is not reflected is
//! transmitted, because the film does not absorb.

use std::f64::consts::TAU;

fn airy(r: f64, sin2_beta: f64) -> f64 {
    let gap = 1.0 - r * r;
    if gap < 1e-12 {
        return 1.0;
    }
    let f = 4.0 * r * r / (gap * gap);
    f * sin2_beta / (1.0 + f * sin2_beta)
}

/// Unpolarized reflectance of a film of refractive index `film_index` and thickness
/// `thickness_nm`, for light of wavelength `nm` arriving at an angle whose cosine is `cos_i`.
pub fn thin_film_reflectance(cos_i: f64, film_index: f64, thickness_nm: f64, nm: f64) -> f64 {
    if thickness_nm <= 0.0 || nm <= 0.0 {
        return 0.0;
    }
    let cos_i = cos_i.clamp(0.0, 1.0);
    let sin2_t = (1.0 - cos_i * cos_i) / (film_index * film_index);
    let cos_t = (1.0 - sin2_t).max(0.0).sqrt();
    let beta = TAU * film_index * thickness_nm * cos_t / nm;
    let sin2_beta = beta.sin().powi(2);
    let rs = (cos_i - film_index * cos_t) / (cos_i + film_index * cos_t);
    let rp = (film_index * cos_i - cos_t) / (film_index * cos_i + cos_t);
    f64::midpoint(airy(rs, sin2_beta), airy(rp, sin2_beta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenefile::SceneFile;

    const N: f64 = 1.33;

    fn r_normal() -> f64 {
        (1.0 - N) / (1.0 + N)
    }

    #[test]
    fn zero_thickness_does_not_reflect() {
        assert!(thin_film_reflectance(1.0, N, 0.0, 550.0).abs() < 1e-15);
        assert!(thin_film_reflectance(0.5, N, 0.0, 550.0).abs() < 1e-15);
    }

    #[test]
    fn quarter_wave_film_reflects_the_most() {
        let r2 = r_normal() * r_normal();
        let expected = 4.0 * r2 / ((1.0 + r2) * (1.0 + r2));
        let d = 550.0 / (4.0 * N);
        assert!((thin_film_reflectance(1.0, N, d, 550.0) - expected).abs() < 1e-12);
    }

    #[test]
    fn half_wave_film_is_invisible() {
        let d = 550.0 / (2.0 * N);
        assert!(thin_film_reflectance(1.0, N, d, 550.0) < 1e-12);
    }

    #[test]
    fn averaging_over_thickness_gives_the_incoherent_sum() {
        let r2 = r_normal() * r_normal();
        let steps = 2000;
        let period = 550.0 / (2.0 * N);
        let mean = (0..steps)
            .map(|k| {
                let d = (f64::from(k) + 0.5) / f64::from(steps) * period;
                thin_film_reflectance(1.0, N, d, 550.0)
            })
            .sum::<f64>()
            / f64::from(steps);
        assert!((mean - 2.0 * r2 / (1.0 + r2)).abs() < 1e-9, "mean = {mean}");
    }

    #[test]
    fn grazing_incidence_is_nearly_a_mirror() {
        assert!(thin_film_reflectance(0.001, N, 300.0, 550.0) > 0.99);
    }

    #[test]
    fn thickness_gives_colour() {
        let blue = thin_film_reflectance(1.0, N, 250.0, 450.0);
        let red = thin_film_reflectance(1.0, N, 250.0, 650.0);
        assert!(blue - red > 0.05, "blue {blue}, red {red}");
    }

    #[test]
    fn reflectance_stays_between_zero_and_one() {
        for c in 0..=20 {
            for d in 0..=40 {
                for l in 0..=8 {
                    let cos_i = f64::from(c) / 20.0;
                    let thickness = f64::from(d) * 25.0;
                    let nm = 380.0 + f64::from(l) * 50.0;
                    let r = thin_film_reflectance(cos_i, N, thickness, nm);
                    assert!(
                        r.is_finite() && (0.0..=1.0).contains(&r),
                        "R = {r} at cos {cos_i}, {thickness} nm film, {nm} nm light"
                    );
                }
            }
        }
    }

    #[test]
    fn scene_files_accept_thin_film_materials() {
        let head = "camera 0 1 5 0 0 0 40\n";
        let ok = SceneFile::parse(&format!("{head}sphere 0 1 0 1 thinfilm 1.33 280\n"));
        assert_eq!(ok.expect("valid scene").scene.primitive_count(), 1);
        assert!(SceneFile::parse(&format!("{head}sphere 0 1 0 1 thinfilm 0.5 280\n")).is_err());
        assert!(SceneFile::parse(&format!("{head}sphere 0 1 0 1 thinfilm 1.33 -5\n")).is_err());
        assert!(SceneFile::parse(&format!("{head}sphere 0 1 0 1 thinfilm 1.33\n")).is_err());
    }

    #[test]
    fn bubble_example_parses() {
        let file = SceneFile::parse(include_str!("../../../examples/bubble.scene")).expect("valid");
        assert_eq!(file.scene.primitive_count(), 16 * 16 * 2 + 3);
    }
}
