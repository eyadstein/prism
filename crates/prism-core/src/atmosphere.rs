//! A physically motivated sky, computed one wavelength at a time.
//!
//! Sunlight (a 5778 K blackbody) is scattered by air molecules. The Rayleigh optical depth varies
//! as the inverse fourth power of the wavelength, so blue light scatters far more than red and
//! the sky comes out blue, whiter towards the horizon, and sunsets turn red, without any colour
//! being chosen by hand.
//!
//! The scattered radiance uses the single-scattering formula for a plane-parallel slab,
//! `L = G * P(g) * mu_s / (mu_s + mu_v) * (1 - exp(-tau * (1 / mu_s + 1 / mu_v)))`, with the
//! Rayleigh phase function `P`. Two simplifications are made on purpose: the view elevation is
//! floored so the horizon does not blow up, and the sunlight reaching the scattering layer is
//! attenuated by half of its path through the air (an empirical correction that produces the red
//! sunset glow, which the plain slab formula cannot). A soft aureole around the sun stands in for
//! the sun disc, which the path tracer cannot sample directly.
//!
//! Directions follow the scene convention: +y is up, and the sun azimuth is measured from -z
//! towards +x.

use crate::error::{PrismError, Result};
use crate::math::Vec3;

const PLANCK: f64 = 6.62607015e-34;
const LIGHT_SPEED: f64 = 299792458.0;
const BOLTZMANN: f64 = 1.380649e-23;
const SUN_TEMPERATURE: f64 = 5778.0;
const REFERENCE_NM: f64 = 555.0;
const SKY_GAIN: f64 = 2.2;
const VIEW_FLOOR: f64 = 0.25;
const SUN_FLOOR: f64 = 0.01;
const GLOW_WEIGHT: f64 = 2.5;
const GLOW_WIDTH: f64 = 0.02;
const MIN_ELEVATION: f64 = 0.5;

/// Planck's law: spectral radiance of a blackbody at `kelvin`, in W per steradian per cubic
/// metre, at `nm` nanometres. Returns 0 for non-positive inputs.
pub fn planck_radiance(nm: f64, kelvin: f64) -> f64 {
    if !(nm.is_finite() && nm > 0.0 && kelvin.is_finite() && kelvin > 0.0) {
        return 0.0;
    }
    let lambda = nm * 1.0e-9;
    let exponent = PLANCK * LIGHT_SPEED / (lambda * BOLTZMANN * kelvin);
    2.0 * PLANCK * LIGHT_SPEED * LIGHT_SPEED / lambda.powi(5) / exponent.exp_m1()
}

/// Rayleigh optical depth of the whole atmosphere at sea level for light of `nm` nanometres.
pub fn rayleigh_optical_depth(nm: f64) -> f64 {
    let um = nm / 1000.0;
    let inverse_square = 1.0 / (um * um);
    0.008569
        * inverse_square
        * inverse_square
        * (1.0 + 0.0113 * inverse_square + 0.00013 * inverse_square * inverse_square)
}

/// A sky for a given sun position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    sun_direction: Vec3,
    sun_cos_zenith: f64,
    sun_norm: f64,
    exposure: f64,
}

impl Atmosphere {
    /// Creates a sky. `sun_elevation_deg` is between 0.5 and 90, `sun_azimuth_deg` is measured
    /// from -z towards +x, and `exposure` scales the radiance (it must be positive).
    pub fn new(sun_elevation_deg: f64, sun_azimuth_deg: f64, exposure: f64) -> Result<Self> {
        let bad = |name: &'static str, reason: &str| PrismError::InvalidParameter {
            name,
            reason: reason.to_owned(),
        };
        if !(MIN_ELEVATION..=90.0).contains(&sun_elevation_deg) {
            return Err(bad("sun_elevation", "must be between 0.5 and 90 degrees"));
        }
        if !sun_azimuth_deg.is_finite() {
            return Err(bad("sun_azimuth", "must be a number"));
        }
        if !(exposure.is_finite() && exposure > 0.0) {
            return Err(bad("exposure", "must be a positive number"));
        }
        let elevation = sun_elevation_deg.to_radians();
        let azimuth = sun_azimuth_deg.to_radians();
        let sun_direction = Vec3::new(
            azimuth.sin() * elevation.cos(),
            elevation.sin(),
            -azimuth.cos() * elevation.cos(),
        );
        Ok(Self {
            sun_direction,
            sun_cos_zenith: elevation.sin().max(SUN_FLOOR),
            sun_norm: 1.0 / planck_radiance(REFERENCE_NM, SUN_TEMPERATURE),
            exposure,
        })
    }

    /// Unit vector pointing at the sun.
    pub fn sun_direction(&self) -> Vec3 {
        self.sun_direction
    }

    /// Sky radiance seen along `dir` at wavelength `nm`.
    pub fn radiance(&self, dir: Vec3, nm: f64) -> f64 {
        let view = dir.normalized();
        let mu_v = view.y.max(VIEW_FLOOR);
        let mu_s = self.sun_cos_zenith;
        let cos_gamma = view.dot(self.sun_direction).clamp(-1.0, 1.0);
        let tau = rayleigh_optical_depth(nm);
        let phase = 0.75 * (1.0 + cos_gamma * cos_gamma);
        let optical = tau * (1.0 / mu_s + 1.0 / mu_v);
        let scatter = mu_s / (mu_s + mu_v) * -(-optical).exp_m1();
        let sunlight = (-0.5 * tau / mu_s).exp();
        let glow = GLOW_WEIGHT * ((cos_gamma - 1.0) / GLOW_WIDTH).exp() * (-tau / mu_s).exp();
        self.exposure
            * planck_radiance(nm, SUN_TEMPERATURE)
            * self.sun_norm
            * (SKY_GAIN * phase * scatter * sunlight + glow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrator::radiance;
    use crate::material::Material;
    use crate::math::{Ray, Rng};
    use crate::render::{render, RenderSettings};
    use crate::scene::{Scene, Sky};
    use crate::scenefile::SceneFile;

    fn sky_at(elevation: f64) -> Atmosphere {
        Atmosphere::new(elevation, 0.0, 1.0).expect("valid sky")
    }

    fn up() -> Vec3 {
        Vec3::new(0.0, 1.0, 0.0)
    }

    #[test]
    fn planck_peaks_where_wiens_law_says() {
        let peak = (3800..=7800)
            .map(|i| f64::from(i) / 10.0)
            .max_by(|a, b| planck_radiance(*a, 5778.0).total_cmp(&planck_radiance(*b, 5778.0)))
            .expect("range is not empty");
        assert!((peak - 501.5).abs() < 2.0, "peak at {peak} nm");
    }

    #[test]
    fn planck_matches_a_hand_computed_value() {
        let value = planck_radiance(500.0, 5778.0);
        assert!((value / 2.6376e13 - 1.0).abs() < 0.01, "{value}");
    }

    #[test]
    fn hot_bodies_are_bluer_and_cool_bodies_redder() {
        assert!(planck_radiance(450.0, 9000.0) / planck_radiance(650.0, 9000.0) > 1.5);
        assert!(planck_radiance(700.0, 3000.0) > planck_radiance(500.0, 3000.0));
        assert_eq!(planck_radiance(500.0, 0.0), 0.0);
        assert_eq!(planck_radiance(-5.0, 5778.0), 0.0);
        assert_eq!(planck_radiance(f64::NAN, 5778.0), 0.0);
    }

    #[test]
    fn rayleigh_depth_follows_the_inverse_fourth_power() {
        assert!((rayleigh_optical_depth(550.0) - 0.0973).abs() < 0.001);
        let ratio = rayleigh_optical_depth(450.0) / rayleigh_optical_depth(650.0);
        assert!((4.0..5.0).contains(&ratio), "ratio {ratio}");
        assert!(rayleigh_optical_depth(400.0) > rayleigh_optical_depth(700.0));
    }

    #[test]
    fn the_zenith_is_blue() {
        let sky = sky_at(45.0);
        assert!(sky.radiance(up(), 450.0) > 2.5 * sky.radiance(up(), 650.0));
    }

    #[test]
    fn the_horizon_is_brighter_and_whiter_than_the_zenith() {
        let sky = sky_at(45.0);
        let away = Vec3::new(0.0, 0.05, 1.0);
        assert!(sky.radiance(away, 550.0) > 2.0 * sky.radiance(up(), 550.0));
        let zenith_ratio = sky.radiance(up(), 450.0) / sky.radiance(up(), 650.0);
        let horizon_ratio = sky.radiance(away, 450.0) / sky.radiance(away, 650.0);
        assert!(
            horizon_ratio < zenith_ratio,
            "{horizon_ratio} versus {zenith_ratio}"
        );
    }

    #[test]
    fn a_low_sun_reddens_the_glow_around_it() {
        let sky = sky_at(2.0);
        let towards = sky.sun_direction();
        assert!(sky.radiance(towards, 650.0) > 5.0 * sky.radiance(towards, 450.0));
    }

    #[test]
    fn a_low_sun_dims_the_sky_overhead() {
        let dim = sky_at(3.0).radiance(up(), 550.0);
        let bright = sky_at(60.0).radiance(up(), 550.0);
        assert!(dim < 0.2 * bright, "{dim} versus {bright}");
    }

    #[test]
    fn exposure_scales_the_radiance_linearly() {
        let one = Atmosphere::new(30.0, 10.0, 1.0).expect("valid sky");
        let more = Atmosphere::new(30.0, 10.0, 2.5).expect("valid sky");
        let dir = Vec3::new(0.3, 0.4, -0.5);
        let a = one.radiance(dir, 520.0);
        let b = more.radiance(dir, 520.0);
        assert!((b - 2.5 * a).abs() < 1e-12 * b.abs());
    }

    #[test]
    fn the_sun_direction_follows_elevation_and_azimuth() {
        let front = Atmosphere::new(30.0, 0.0, 1.0).expect("valid sky");
        assert!(front
            .sun_direction()
            .near(Vec3::new(0.0, 0.5, -0.866_025_403_784), 1e-9));
        let side = Atmosphere::new(30.0, 90.0, 1.0).expect("valid sky");
        assert!(side
            .sun_direction()
            .near(Vec3::new(0.866_025_403_784, 0.5, 0.0), 1e-9));
        assert!((front.sun_direction().length() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn radiance_is_finite_and_not_negative_everywhere() {
        let sky = sky_at(20.0);
        let mut rng = Rng::new(4);
        for _ in 0..500 {
            let dir = rng.unit_vector();
            let nm = rng.range(380.0, 780.0);
            let r = sky.radiance(dir, nm);
            assert!(r.is_finite() && r >= 0.0, "{r} for {dir:?} at {nm} nm");
        }
    }

    #[test]
    fn invalid_skies_are_rejected() {
        let name_of = |e: Option<PrismError>| match e {
            Some(PrismError::InvalidParameter { name, .. }) => name,
            _ => "",
        };
        assert_eq!(
            name_of(Atmosphere::new(0.0, 0.0, 1.0).err()),
            "sun_elevation"
        );
        assert_eq!(
            name_of(Atmosphere::new(95.0, 0.0, 1.0).err()),
            "sun_elevation"
        );
        assert_eq!(
            name_of(Atmosphere::new(f64::NAN, 0.0, 1.0).err()),
            "sun_elevation"
        );
        assert_eq!(
            name_of(Atmosphere::new(30.0, f64::NAN, 1.0).err()),
            "sun_azimuth"
        );
        assert_eq!(name_of(Atmosphere::new(30.0, 0.0, 0.0).err()), "exposure");
        assert_eq!(
            name_of(Atmosphere::new(30.0, 0.0, f64::INFINITY).err()),
            "exposure"
        );
        assert!(Atmosphere::new(0.5, -720.0, 0.001).is_ok());
        assert!(Atmosphere::new(90.0, 360.0, 1.0).is_ok());
    }

    #[test]
    fn scenes_use_the_atmosphere_only_when_it_is_set() {
        let mut scene = Scene::new(Sky::uniform(0.3));
        let dir = Vec3::new(0.2, 0.7, -0.4).normalized();
        assert!((scene.background(dir, 520.0) - 0.3).abs() < 1e-12);
        let sky = sky_at(40.0);
        scene.atmosphere = Some(sky);
        assert!((scene.background(dir, 520.0) - sky.radiance(dir, 520.0)).abs() < 1e-12);
    }

    #[test]
    fn the_integrator_uses_the_atmosphere_for_escaping_rays() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        let sky = sky_at(40.0);
        scene.atmosphere = Some(sky);
        let mut rng = Rng::new(1);
        let dir = Vec3::new(0.2, 0.7, -0.4).normalized();
        let l = radiance(&scene, Ray::new(Vec3::ZERO, dir), 520.0, &mut rng, 4);
        assert!((l - sky.radiance(dir, 520.0)).abs() < 1e-12);
    }

    #[test]
    fn a_mirror_reflects_the_sky() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.atmosphere = Some(sky_at(40.0));
        scene.add(
            Material::Mirror { reflectance: 1.0 },
            crate::geometry::Primitive::Sphere(crate::geometry::Sphere::new(
                Vec3::new(0.0, 0.0, -5.0),
                1.0,
            )),
        );
        let mut rng = Rng::new(2);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let l = radiance(&scene, ray, 450.0, &mut rng, 4);
        let expected = sky_at(40.0).radiance(Vec3::new(0.0, 0.0, 1.0), 450.0);
        assert!((l - expected).abs() < 1e-12, "{l} versus {expected}");
    }

    #[test]
    fn scene_files_accept_the_atmosphere_directive() {
        let head = "camera 0 1 5 0 0 0 40\n";
        let full = SceneFile::parse(&format!("{head}atmosphere 30 15 1.5\n")).expect("valid scene");
        assert!(full.scene.atmosphere.is_some());
        let short = SceneFile::parse(&format!("{head}atmosphere 30\n")).expect("valid scene");
        assert!(short.scene.atmosphere.is_some());
        let none = SceneFile::parse(head).expect("valid scene");
        assert!(none.scene.atmosphere.is_none());
        let bad = [
            "atmosphere\n",
            "atmosphere 0\n",
            "atmosphere 95\n",
            "atmosphere 30 x\n",
            "atmosphere 30 0 0\n",
            "atmosphere 30 0 1 7\n",
        ];
        for text in bad {
            assert!(
                SceneFile::parse(&format!("{head}{text}")).is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn daylight_example_renders_a_blue_sky() {
        let file = SceneFile::parse(include_str!("../../../examples/daylight.scene"))
            .expect("valid scene");
        assert!(file.scene.atmosphere.is_some());
        let (w, h) = (24, 14);
        let camera = file.camera.camera(24.0 / 14.0);
        let settings = RenderSettings {
            width: w,
            height: h,
            samples: 16,
            max_depth: 4,
            seed: 9,
        };
        let image = render(&file.scene, &camera, &settings);
        let mut sum = Vec3::ZERO;
        for y in 0..3 {
            for x in 0..w {
                sum += image.pixels[y * w + x];
            }
        }
        let mean = sum / (3 * w) as f64;
        assert!(mean.z > 1.3 * mean.x, "sky colour {mean:?}");
        assert!(mean.y.is_finite() && mean.y > 0.05, "sky colour {mean:?}");
    }

    #[test]
    fn sunset_example_renders() {
        let file =
            SceneFile::parse(include_str!("../../../examples/sunset.scene")).expect("valid scene");
        assert!(file.scene.atmosphere.is_some());
        let camera = file.camera.camera(16.0 / 9.0);
        let settings = RenderSettings {
            width: 16,
            height: 9,
            samples: 4,
            max_depth: 4,
            seed: 3,
        };
        let mean = render(&file.scene, &camera, &settings).mean();
        assert!(mean.x.is_finite() && mean.x > 0.001, "mean {mean:?}");
    }
}
