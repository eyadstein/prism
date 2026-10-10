//! Cameras: the pinhole camera, and the trait the renderer uses to ask any camera for rays.

use crate::math::{Ray, Rng, Vec3};

/// A camera ray together with the factor that scales the radiance seen along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSample {
    /// Ray into the scene.
    pub ray: Ray,
    /// Weight of the radiance seen along the ray; 1 for an ideal pinhole camera.
    pub weight: f64,
}

/// Anything that turns a film position into rays for the renderer.
pub trait RaySource: Sync {
    /// Ray for the film point `(s, t)`, both in `[0, 1]` with `s` running left to right and
    /// `t` bottom to top, at wavelength `nm` nanometres. `rng` supplies random numbers for
    /// lens sampling. Returns `None` when the ray is blocked.
    fn sample(&self, s: f64, t: f64, nm: f64, rng: &mut Rng) -> Option<CameraSample>;
}

/// A pinhole camera with a vertical field of view.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    origin: Vec3,
    lower_left: Vec3,
    horizontal: Vec3,
    vertical: Vec3,
}

impl Camera {
    /// Creates a camera at `eye` looking at `target`. `aspect` is width divided by height.
    pub fn new(eye: Vec3, target: Vec3, up: Vec3, vfov_degrees: f64, aspect: f64) -> Self {
        let half_h = (vfov_degrees.to_radians() * 0.5).tan();
        let half_w = aspect * half_h;
        let w = (eye - target).normalized();
        let u = up.cross(w).normalized();
        let v = w.cross(u);
        let horizontal = u * (2.0 * half_w);
        let vertical = v * (2.0 * half_h);
        Self {
            origin: eye,
            lower_left: eye - horizontal * 0.5 - vertical * 0.5 - w,
            horizontal,
            vertical,
        }
    }

    /// Ray through the film point `(s, t)`; both in `[0, 1]`, `s` left to right, `t` bottom to top.
    pub fn ray(&self, s: f64, t: f64) -> Ray {
        let target = self.lower_left + self.horizontal * s + self.vertical * t;
        Ray::new(self.origin, (target - self.origin).normalized())
    }
}

impl RaySource for Camera {
    fn sample(&self, s: f64, t: f64, _nm: f64, _rng: &mut Rng) -> Option<CameraSample> {
        Some(CameraSample {
            ray: self.ray(s, t),
            weight: 1.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up() -> Vec3 {
        Vec3::new(0.0, 1.0, 0.0)
    }

    #[test]
    fn centre_ray_points_at_target() {
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let target = Vec3::new(4.0, -1.0, 2.0);
        let cam = Camera::new(eye, target, up(), 50.0, 1.7);
        let r = cam.ray(0.5, 0.5);
        assert!(r.origin.near(eye, 1e-12));
        assert!(r.dir.near((target - eye).normalized(), 1e-12));
    }

    #[test]
    fn field_of_view_sets_edge_angle() {
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0), up(), 90.0, 1.0);
        let top = cam.ray(0.5, 1.0).dir;
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!(top.near(Vec3::new(0.0, s, -s), 1e-12));
    }

    #[test]
    fn left_is_negative_x_and_right_is_positive_x() {
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), up(), 60.0, 2.0);
        assert!(cam.ray(0.0, 0.5).dir.x < 0.0);
        assert!(cam.ray(1.0, 0.5).dir.x > 0.0);
        assert!((cam.ray(0.3, 0.8).dir.length() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pinhole_samples_match_the_ray_method() {
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), up(), 60.0, 1.5);
        let mut rng = Rng::new(1);
        let sample = cam
            .sample(0.3, 0.8, 550.0, &mut rng)
            .expect("pinhole rays are never blocked");
        assert_eq!(sample.ray, cam.ray(0.3, 0.8));
        assert!((sample.weight - 1.0).abs() < 1e-15);
    }
}
