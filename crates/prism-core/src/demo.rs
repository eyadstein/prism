//! A built-in showcase scene: glass spheres of different dispersion over a checkerboard.

use crate::camera::Camera;
use crate::geometry::{Primitive, Sphere, Triangle};
use crate::glass::{BK7, DIAMOND, SF11};
use crate::material::Material;
use crate::math::Vec3;
use crate::scene::{Scene, Sky};

/// Builds the demo scene and a camera framing it for the given aspect ratio.
pub fn demo_scene(aspect: f64) -> (Scene, Camera) {
    let mut scene = Scene::new(Sky {
        horizon: 1.0,
        zenith: 0.45,
    });

    let mut light = Vec::new();
    let mut dark = Vec::new();
    let half = 8_i32;
    for i in -half..half {
        for j in -half..half {
            let (x, z) = (f64::from(i), f64::from(j));
            let a = Vec3::new(x, 0.0, z);
            let b = Vec3::new(x + 1.0, 0.0, z);
            let c = Vec3::new(x + 1.0, 0.0, z + 1.0);
            let d = Vec3::new(x, 0.0, z + 1.0);
            let tiles = if (i + j).rem_euclid(2) == 0 {
                &mut light
            } else {
                &mut dark
            };
            tiles.push(Primitive::Triangle(Triangle::new(a, b, c)));
            tiles.push(Primitive::Triangle(Triangle::new(a, c, d)));
        }
    }
    scene.add_group(Material::Diffuse { reflectance: 0.85 }, light);
    scene.add_group(Material::Diffuse { reflectance: 0.04 }, dark);

    let ball =
        |x: f64, y: f64, z: f64, r: f64| Primitive::Sphere(Sphere::new(Vec3::new(x, y, z), r));
    scene.add(Material::Dielectric(&SF11), ball(0.0, 1.0, 0.0, 1.0));
    scene.add(Material::Dielectric(&DIAMOND), ball(2.3, 0.55, 1.0, 0.55));
    scene.add(Material::Dielectric(&BK7), ball(-2.2, 0.7, 0.8, 0.7));
    scene.add(
        Material::Mirror { reflectance: 0.92 },
        ball(-0.8, 0.45, 2.4, 0.45),
    );

    let camera = Camera::new(
        Vec3::new(0.0, 1.8, 6.5),
        Vec3::new(0.0, 0.7, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        38.0,
        aspect,
    );
    (scene, camera)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{render, RenderSettings};

    #[test]
    fn scene_has_expected_geometry() {
        let (scene, _) = demo_scene(16.0 / 9.0);
        assert_eq!(scene.primitive_count(), 16 * 16 * 2 + 4);
    }

    #[test]
    fn small_render_is_finite_and_not_black() {
        let (scene, camera) = demo_scene(16.0 / 9.0);
        let settings = RenderSettings {
            width: 16,
            height: 9,
            samples: 2,
            max_depth: 8,
            seed: 7,
        };
        let img = render(&scene, &camera, &settings);
        let mean = img.mean();
        assert!(mean.x.is_finite() && mean.y.is_finite() && mean.z.is_finite());
        assert!(mean.x > 0.01 && mean.y > 0.01 && mean.z > 0.01);
    }
}
