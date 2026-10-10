//! A camera that looks through a real lens.
//!
//! Rays start at a point on a sensor behind the lens, aim at a random point of the lens's rear
//! aperture, and are traced through every surface at the wavelength of the sample. Depth of
//! field, chromatic aberration, vignetting and out-of-focus highlights therefore come out of
//! the optics instead of being painted on. Each ray is weighted by `cos^4` of its angle to the
//! axis at the sensor, which gives the natural brightness falloff towards the corners.
//!
//! Scene units are metres and the lens prescription is in millimetres.

use crate::camera::{CameraSample, RaySource};
use crate::error::{PrismError, Result};
use crate::lens::{Lens, D_LINE_NM};
use crate::math::{Ray, Rng, Vec3};

const MM_PER_UNIT: f64 = 1000.0;

/// A lens, a sensor, and a position in the scene.
#[derive(Clone, Debug)]
pub struct LensCamera {
    reversed: Lens,
    sensor_z: f64,
    half_width: f64,
    half_height: f64,
    aperture: f64,
    front_z: f64,
    eye: Vec3,
    x_axis: Vec3,
    y_axis: Vec3,
    z_axis: Vec3,
}

impl LensCamera {
    /// Places `lens` with its front vertex at `eye`, pointing at `target`, focused at
    /// `focus_distance` scene units (metres) in front of the lens. `sensor_width_mm` is the
    /// width of the sensor and `aspect` the image width divided by its height. The last
    /// surface of the lens must have a finite semi-aperture.
    pub fn new(
        lens: &Lens,
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        focus_distance: f64,
        sensor_width_mm: f64,
        aspect: f64,
    ) -> Result<Self> {
        let bad = |name: &'static str, reason: &str| PrismError::InvalidParameter {
            name,
            reason: reason.to_owned(),
        };
        if !(focus_distance.is_finite() && focus_distance > 0.0) {
            return Err(bad("focus_distance", "must be a positive number"));
        }
        if !(sensor_width_mm.is_finite() && sensor_width_mm > 0.0) {
            return Err(bad("sensor_width", "must be a positive number"));
        }
        if !(aspect.is_finite() && aspect > 0.0) {
            return Err(bad("aspect", "must be a positive number"));
        }
        let forward = (target - eye).normalized();
        let right = forward.cross(up).normalized();
        if forward.length_squared() < 0.5 || right.length_squared() < 0.5 {
            return Err(bad(
                "camera",
                "must look at a different point and not straight up or down",
            ));
        }
        let image_distance = lens
            .image_distance(D_LINE_NM, focus_distance * MM_PER_UNIT)
            .filter(|d| d.is_finite() && *d > 0.0)
            .ok_or_else(|| bad("focus_distance", "the lens cannot focus at that distance"))?;
        let reversed = lens.reversed();
        let aperture = reversed.surfaces()[0].semi_aperture;
        if !(aperture.is_finite() && aperture > 0.0) {
            return Err(bad(
                "semi_aperture",
                "the last surface needs a finite aperture",
            ));
        }
        let front_z = reversed.surfaces().iter().map(|s| s.thickness).sum();
        Ok(Self {
            reversed,
            sensor_z: -image_distance,
            half_width: sensor_width_mm / 2.0,
            half_height: sensor_width_mm / (2.0 * aspect),
            aperture,
            front_z,
            eye,
            x_axis: -right,
            y_axis: right.cross(forward),
            z_axis: forward,
        })
    }

    fn local_to_world(&self, v: Vec3) -> Vec3 {
        self.x_axis * v.x + self.y_axis * v.y + self.z_axis * v.z
    }
}

impl RaySource for LensCamera {
    fn sample(&self, s: f64, t: f64, nm: f64, rng: &mut Rng) -> Option<CameraSample> {
        let sensor = Vec3::new(
            (s - 0.5) * 2.0 * self.half_width,
            -(t - 0.5) * 2.0 * self.half_height,
            self.sensor_z,
        );
        let (ax, ay) = loop {
            let x = rng.range(-1.0, 1.0);
            let y = rng.range(-1.0, 1.0);
            if x * x + y * y <= 1.0 {
                break (x * self.aperture, y * self.aperture);
            }
        };
        let dir = (Vec3::new(ax, ay, 0.0) - sensor).normalized();
        let out = self.reversed.trace(Ray::new(sensor, dir), nm).ok()?;
        if out.dir.z <= 0.0 {
            return None;
        }
        let local = Vec3::new(out.origin.x, out.origin.y, out.origin.z - self.front_z);
        let origin = self.eye + self.local_to_world(local) / MM_PER_UNIT;
        Some(CameraSample {
            ray: Ray::new(origin, self.local_to_world(out.dir).normalized()),
            weight: dir.z.powi(4),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::film::Image;
    use crate::geometry::{Primitive, Sphere};
    use crate::material::Material;
    use crate::render::{render, RenderSettings};
    use crate::scene::{Scene, Sky};

    fn doublet() -> Lens {
        Lens::parse(include_str!("../../../examples/doublet.lens")).expect("valid lens")
    }

    fn camera_for(lens: &Lens, focus: f64, aspect: f64) -> LensCamera {
        LensCamera::new(
            lens,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
            focus,
            36.0,
            aspect,
        )
        .expect("valid camera")
    }

    fn settings(width: usize, height: usize, samples: u32) -> RenderSettings {
        RenderSettings {
            width,
            height,
            samples,
            max_depth: 4,
            seed: 3,
        }
    }

    fn dark_sphere(center: Vec3, radius: f64) -> Scene {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.add(
            Material::Diffuse { reflectance: 0.0 },
            Primitive::Sphere(Sphere::new(center, radius)),
        );
        scene
    }

    fn region_mean(image: &Image, x0: usize, y0: usize, w: usize, h: usize) -> f64 {
        let mut sum = 0.0;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                sum += image.pixels[y * image.width + x].y;
            }
        }
        sum / (w * h) as f64
    }

    #[test]
    fn a_sphere_on_the_right_appears_on_the_right() {
        let camera = camera_for(&doublet(), 10.0, 32.0 / 18.0);
        let scene = dark_sphere(Vec3::new(0.9, 0.0, -10.0), 0.8);
        let image = render(&scene, &camera, &settings(32, 18, 24));
        let left = region_mean(&image, 0, 0, 16, 18);
        let right = region_mean(&image, 16, 0, 16, 18);
        assert!(left > 0.8, "left {left}");
        assert!(right < 0.8 * left, "left {left}, right {right}");
    }

    #[test]
    fn a_sphere_above_the_axis_appears_in_the_upper_half() {
        let camera = camera_for(&doublet(), 10.0, 32.0 / 18.0);
        let scene = dark_sphere(Vec3::new(0.0, 0.95, -10.0), 0.9);
        let image = render(&scene, &camera, &settings(32, 18, 24));
        let top = region_mean(&image, 0, 0, 32, 9);
        let bottom = region_mean(&image, 0, 9, 32, 9);
        assert!(bottom > 0.8, "bottom {bottom}");
        assert!(top < 0.8 * bottom, "top {top}, bottom {bottom}");
    }

    #[test]
    fn the_focus_distance_controls_sharpness() {
        let lens = doublet();
        let scene = dark_sphere(Vec3::new(0.0, 0.0, -10.0), 0.03);
        let aspect = 16.0 / 9.0;
        let sharp = render(
            &scene,
            &camera_for(&lens, 10.0, aspect),
            &settings(128, 72, 48),
        );
        let soft = render(
            &scene,
            &camera_for(&lens, 3.0, aspect),
            &settings(128, 72, 48),
        );
        let darkest = |image: &Image| {
            image
                .pixels
                .iter()
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min)
        };
        let (in_focus, out_of_focus) = (darkest(&sharp), darkest(&soft));
        assert!(in_focus < 0.6, "in focus {in_focus}");
        assert!(
            out_of_focus > 1.5 * in_focus,
            "in focus {in_focus}, out of focus {out_of_focus}"
        );
    }

    #[test]
    fn empty_scenes_fall_off_towards_the_corners() {
        let camera = camera_for(&doublet(), 10.0, 16.0 / 9.0);
        let image = render(
            &Scene::new(Sky::uniform(1.0)),
            &camera,
            &settings(64, 36, 48),
        );
        let centre = region_mean(&image, 30, 16, 4, 4);
        let corner = region_mean(&image, 0, 0, 4, 4);
        assert!(centre > 0.9, "centre {centre}");
        assert!(corner < centre, "corner {corner}, centre {centre}");
        assert!(corner > 0.6 * centre, "corner {corner}, centre {centre}");
    }

    #[test]
    fn a_tiny_front_aperture_blocks_most_rays() {
        let lens = Lens::parse("44.78 4.0 N-BK7 1.0\n-44.78 2.5 F2 12.5\n-812 95.0 air 12.5\n")
            .expect("valid lens");
        let camera = camera_for(&lens, 10.0, 16.0 / 9.0);
        let mut rng = Rng::new(1);
        let passed = (0..2000)
            .filter(|_| camera.sample(0.5, 0.5, 550.0, &mut rng).is_some())
            .count();
        assert!(passed < 200, "{passed} rays passed");
    }

    #[test]
    fn centre_rays_leave_the_lens_towards_the_subject() {
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let camera = LensCamera::new(
            &doublet(),
            eye,
            Vec3::new(1.0, 2.0, -7.0),
            Vec3::new(0.0, 1.0, 0.0),
            10.0,
            36.0,
            16.0 / 9.0,
        )
        .expect("valid camera");
        let mut rng = Rng::new(5);
        let mut seen = 0;
        for _ in 0..50 {
            if let Some(c) = camera.sample(0.5, 0.5, 550.0, &mut rng) {
                seen += 1;
                assert!(
                    c.ray.dir.dot(Vec3::new(0.0, 0.0, -1.0)) > 0.99,
                    "{:?}",
                    c.ray.dir
                );
                assert!((c.ray.origin - eye).length() < 0.03, "{:?}", c.ray.origin);
                assert!(c.weight > 0.9 && c.weight <= 1.0 + 1e-12, "{}", c.weight);
            }
        }
        assert!(seen > 40, "only {seen} of 50 rays passed");
    }

    #[test]
    fn rejects_unusable_setups() {
        let lens = doublet();
        let up = Vec3::new(0.0, 1.0, 0.0);
        let eye = Vec3::ZERO;
        let ahead = Vec3::new(0.0, 0.0, -1.0);
        let bad = |focus: f64, width: f64, aspect: f64| {
            LensCamera::new(&lens, eye, ahead, up, focus, width, aspect).err()
        };
        assert!(matches!(
            bad(0.0, 36.0, 1.0),
            Some(PrismError::InvalidParameter {
                name: "focus_distance",
                ..
            })
        ));
        assert!(matches!(
            bad(f64::NAN, 36.0, 1.0),
            Some(PrismError::InvalidParameter {
                name: "focus_distance",
                ..
            })
        ));
        assert!(matches!(
            bad(10.0, 0.0, 1.0),
            Some(PrismError::InvalidParameter {
                name: "sensor_width",
                ..
            })
        ));
        assert!(matches!(
            bad(10.0, 36.0, 0.0),
            Some(PrismError::InvalidParameter { name: "aspect", .. })
        ));
        assert!(matches!(
            bad(0.05, 36.0, 1.0),
            Some(PrismError::InvalidParameter {
                name: "focus_distance",
                ..
            })
        ));
        assert!(matches!(
            LensCamera::new(&lens, eye, up, up, 10.0, 36.0, 1.0).err(),
            Some(PrismError::InvalidParameter { name: "camera", .. })
        ));
        let no_aperture = Lens::parse("51.68 5 N-BK7\nflat 90 air\n").expect("valid lens");
        assert!(matches!(
            LensCamera::new(&no_aperture, eye, ahead, up, 10.0, 36.0, 1.0).err(),
            Some(PrismError::InvalidParameter {
                name: "semi_aperture",
                ..
            })
        ));
        let plate = Lens::parse("flat 5 N-BK7 12\nflat 10 air 12\n").expect("valid lens");
        assert!(LensCamera::new(&plate, eye, ahead, up, 10.0, 36.0, 1.0).is_err());
    }

    #[test]
    fn the_example_scene_renders_through_the_example_lens() {
        let file = crate::scenefile::SceneFile::parse(include_str!("../../../examples/lens.scene"))
            .expect("valid scene");
        let camera = LensCamera::new(
            &doublet(),
            file.camera.eye,
            file.camera.target,
            Vec3::new(0.0, 1.0, 0.0),
            5.5,
            36.0,
            16.0 / 9.0,
        )
        .expect("valid camera");
        let image = render(&file.scene, &camera, &settings(32, 18, 4));
        let mean = image.mean();
        assert!(mean.y.is_finite() && mean.y > 0.1, "mean {mean:?}");
    }
}
