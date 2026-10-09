//! Auxiliary per-pixel buffers (albedo, normal, depth) that guide the denoiser.

use crate::camera::Camera;
use crate::material::Material;
use crate::math::Vec3;
use crate::scene::Scene;

const SURFACE_EPSILON: f64 = 1e-6;

/// Noise-free first-hit data for every pixel; row-major with the top row first.
#[derive(Clone, Debug)]
pub struct Features {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Surface reflectance at the first hit: 1 for glass and films, 0 for sky.
    pub albedo: Vec<f64>,
    /// Unit surface normal at the first hit; zero for sky.
    pub normal: Vec<Vec3>,
    /// Distance to the first hit; infinite for sky.
    pub depth: Vec<f64>,
}

/// Casts one ray through the centre of every pixel.
pub fn compute(scene: &Scene, camera: &Camera, width: usize, height: usize) -> Features {
    let n = width * height;
    let mut features = Features {
        width,
        height,
        albedo: Vec::with_capacity(n),
        normal: Vec::with_capacity(n),
        depth: Vec::with_capacity(n),
    };
    for y in 0..height {
        for x in 0..width {
            let px = (x as f64 + 0.5) / width as f64;
            let py = 1.0 - (y as f64 + 0.5) / height as f64;
            let ray = camera.ray(px, py);
            if let Some((hit, material)) = scene.intersect(&ray, SURFACE_EPSILON, f64::INFINITY) {
                let albedo = match material {
                    Material::Diffuse { reflectance } | Material::Mirror { reflectance } => {
                        *reflectance
                    }
                    Material::Dielectric(_) | Material::ThinFilm { .. } => 1.0,
                };
                features.albedo.push(albedo);
                features.normal.push(hit.normal);
                features.depth.push(hit.t);
            } else {
                features.albedo.push(0.0);
                features.normal.push(Vec3::ZERO);
                features.depth.push(f64::INFINITY);
            }
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Primitive, Sphere};
    use crate::glass::BK7;
    use crate::scene::Sky;

    fn camera() -> Camera {
        Camera::new(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
            40.0,
            1.0,
        )
    }

    fn scene_with(material: Material) -> Scene {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.add(material, Primitive::Sphere(Sphere::new(Vec3::ZERO, 1.0)));
        scene
    }

    #[test]
    fn centre_pixel_sees_the_sphere_and_corner_sees_sky() {
        let f = compute(
            &scene_with(Material::Diffuse { reflectance: 0.6 }),
            &camera(),
            5,
            5,
        );
        assert_eq!(f.albedo.len(), 25);
        assert_eq!(f.normal.len(), 25);
        assert_eq!(f.depth.len(), 25);
        let centre = 2 * 5 + 2;
        assert!((f.albedo[centre] - 0.6).abs() < 1e-12);
        assert!((f.depth[centre] - 4.0).abs() < 1e-9);
        assert!(f.normal[centre].near(Vec3::new(0.0, 0.0, 1.0), 1e-9));
        assert!(f.depth[0].is_infinite());
        assert_eq!(f.normal[0], Vec3::ZERO);
        assert!(f.albedo[0].abs() < 1e-12);
    }

    #[test]
    fn albedo_depends_on_the_material() {
        let centre = 2 * 5 + 2;
        let mirror = compute(
            &scene_with(Material::Mirror { reflectance: 0.9 }),
            &camera(),
            5,
            5,
        );
        assert!((mirror.albedo[centre] - 0.9).abs() < 1e-12);
        let glass = compute(&scene_with(Material::Dielectric(&BK7)), &camera(), 5, 5);
        assert!((glass.albedo[centre] - 1.0).abs() < 1e-12);
        let film = Material::ThinFilm {
            index: 1.33,
            thickness_nm: 280.0,
        };
        let bubble = compute(&scene_with(film), &camera(), 5, 5);
        assert!((bubble.albedo[centre] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn empty_images_have_empty_buffers() {
        let f = compute(&scene_with(Material::Dielectric(&BK7)), &camera(), 0, 0);
        assert_eq!(f.albedo.len(), 0);
        assert_eq!(f.depth.len(), 0);
    }
}
