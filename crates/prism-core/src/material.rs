//! Surface materials, Fresnel reflectance, and dispersive dielectric scattering.

use crate::geometry::Hit;
use crate::glass::Glass;
use crate::math::Vec3;

/// A surface material.
#[derive(Clone, Copy, Debug)]
pub enum Material {
    /// Ideal diffuse reflector with scalar reflectance in `[0, 1]`.
    Diffuse {
        /// Fraction of light reflected.
        reflectance: f64,
    },
    /// Perfect mirror with scalar reflectance in `[0, 1]`.
    Mirror {
        /// Fraction of light reflected.
        reflectance: f64,
    },
    /// Transparent dispersive dielectric.
    Dielectric(&'static Glass),
    /// Infinitely thin transparent film (such as a soap bubble wall) in air.
    ThinFilm {
        /// Refractive index of the film.
        index: f64,
        /// Film thickness in nanometres.
        thickness_nm: f64,
    },
}

/// Unpolarized Fresnel reflectance for light going from index `n1` into `n2`.
/// `cos_i` is the cosine of the incident angle. Returns 1 on total internal reflection.
pub fn fresnel_dielectric(cos_i: f64, n1: f64, n2: f64) -> f64 {
    let cos_i = cos_i.clamp(0.0, 1.0);
    let ratio = n1 / n2;
    let sin2_t = ratio * ratio * (1.0 - cos_i * cos_i);
    if sin2_t >= 1.0 {
        return 1.0;
    }
    let cos_t = (1.0 - sin2_t).sqrt();
    let rs = (n1 * cos_i - n2 * cos_t) / (n1 * cos_i + n2 * cos_t);
    let rp = (n1 * cos_t - n2 * cos_i) / (n1 * cos_t + n2 * cos_i);
    f64::midpoint(rs * rs, rp * rp)
}

/// Chooses reflection or refraction at a glass/air interface for wavelength `nm`.
/// `u` is a uniform random number in `[0, 1)`; the ray reflects when `u` is below the
/// Fresnel reflectance. Assumes the other side of the interface is air.
pub fn dielectric_scatter(dir: Vec3, hit: &Hit, glass: &Glass, nm: f64, u: f64) -> Vec3 {
    let d = dir.normalized();
    let n_glass = glass.index(nm);
    let (n1, n2) = if hit.front_face {
        (1.0, n_glass)
    } else {
        (n_glass, 1.0)
    };
    let cos_i = (-d.dot(hit.normal)).clamp(0.0, 1.0);
    if u < fresnel_dielectric(cos_i, n1, n2) {
        d.reflect(hit.normal)
    } else {
        d.refract(hit.normal, n1 / n2)
            .unwrap_or_else(|| d.reflect(hit.normal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glass::{BK7, SF11};

    fn flat_hit(front_face: bool) -> Hit {
        Hit {
            t: 1.0,
            point: Vec3::ZERO,
            normal: Vec3::new(0.0, 0.0, 1.0),
            front_face,
        }
    }

    #[test]
    fn normal_incidence_reflectance() {
        let r = fresnel_dielectric(1.0, 1.0, 1.5);
        assert!((r - 0.04).abs() < 1e-12);
    }

    #[test]
    fn grazing_incidence_is_nearly_a_mirror() {
        assert!(fresnel_dielectric(0.01, 1.0, 1.5) > 0.9);
    }

    #[test]
    fn total_internal_reflection_gives_one() {
        assert_eq!(fresnel_dielectric(0.5, 1.5, 1.0), 1.0);
    }

    #[test]
    fn scatter_reflects_or_refracts_by_random_number() {
        let down = Vec3::new(0.0, 0.0, -1.0);
        let reflected = dielectric_scatter(down, &flat_hit(true), &BK7, 550.0, 0.0);
        assert!(reflected.near(Vec3::new(0.0, 0.0, 1.0), 1e-12));
        let straight = dielectric_scatter(down, &flat_hit(true), &BK7, 550.0, 0.99);
        assert!(straight.near(down, 1e-12));
    }

    #[test]
    fn blue_refracts_closer_to_the_normal_than_red() {
        let s = 0.5_f64;
        let dir = Vec3::new(s, 0.0, -(1.0 - s * s).sqrt());
        let blue = dielectric_scatter(dir, &flat_hit(true), &SF11, 450.0, 0.99);
        let red = dielectric_scatter(dir, &flat_hit(true), &SF11, 650.0, 0.99);
        assert!(blue.x < red.x);
        assert!((blue.length() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn exiting_glass_bends_away_from_the_normal() {
        let s = 0.3_f64;
        let dir = Vec3::new(s, 0.0, -(1.0 - s * s).sqrt());
        let out = dielectric_scatter(dir, &flat_hit(false), &BK7, 550.0, 0.99);
        assert!(out.x > s);
    }
}
