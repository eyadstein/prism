//! Monochromatic path tracing. Each call follows one ray at one wavelength.

use crate::material::{dielectric_scatter, Material};
use crate::math::{Ray, Rng, Vec3};
use crate::scene::Scene;
use crate::thinfilm::thin_film_reflectance;

/// Minimum ray parameter, which avoids re-hitting the surface a ray just left.
const SURFACE_EPSILON: f64 = 1e-6;

/// Cosine-weighted direction in the hemisphere around the unit vector `normal`.
pub fn cosine_hemisphere(normal: Vec3, rng: &mut Rng) -> Vec3 {
    let d = normal + rng.unit_vector();
    if d.length_squared() < 1e-16 {
        normal
    } else {
        d.normalized()
    }
}

/// Radiance arriving along `ray` at wavelength `nm`, estimated with one random path.
pub fn radiance(scene: &Scene, mut ray: Ray, nm: f64, rng: &mut Rng, max_depth: u32) -> f64 {
    let mut throughput = 1.0_f64;
    for depth in 0..max_depth {
        let Some((hit, material)) = scene.intersect(&ray, SURFACE_EPSILON, f64::INFINITY) else {
            return throughput * scene.background(ray.dir, nm);
        };
        let dir = match material {
            Material::Diffuse { reflectance } => {
                throughput *= *reflectance;
                cosine_hemisphere(hit.normal, rng)
            }
            Material::Mirror { reflectance } => {
                throughput *= *reflectance;
                ray.dir.normalized().reflect(hit.normal)
            }
            Material::Dielectric(glass) => {
                dielectric_scatter(ray.dir, &hit, glass, nm, rng.next_f64())
            }
            Material::ThinFilm {
                index,
                thickness_nm,
            } => {
                let d = ray.dir.normalized();
                let cos_i = (-d.dot(hit.normal)).clamp(0.0, 1.0);
                if rng.next_f64() < thin_film_reflectance(cos_i, *index, *thickness_nm, nm) {
                    d.reflect(hit.normal)
                } else {
                    d
                }
            }
        };
        ray = Ray::new(hit.point, dir);
        if depth >= 3 && throughput < 0.1 {
            if rng.next_f64() < 0.5 {
                throughput *= 2.0;
            } else {
                return 0.0;
            }
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Primitive, Sphere};
    use crate::glass::SF11;
    use crate::scene::Sky;

    fn sphere_at_z5() -> Primitive {
        Primitive::Sphere(Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0))
    }

    #[test]
    fn empty_scene_returns_sky() {
        let scene = Scene::new(Sky::uniform(0.7));
        let mut rng = Rng::new(1);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.3, 0.2, -1.0));
        assert!((radiance(&scene, ray, 550.0, &mut rng, 8) - 0.7).abs() < 1e-12);
    }

    #[test]
    fn mirror_reflects_the_sky_behind_the_camera() {
        let mut scene = Scene::new(Sky {
            horizon: 0.8,
            zenith: 0.2,
        });
        scene.add(Material::Mirror { reflectance: 0.9 }, sphere_at_z5());
        let mut rng = Rng::new(1);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let l = radiance(&scene, ray, 550.0, &mut rng, 8);
        assert!((l - 0.72).abs() < 1e-12, "got {l}");
    }

    #[test]
    fn diffuse_sphere_in_a_furnace_returns_its_reflectance() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.add(Material::Diffuse { reflectance: 0.5 }, sphere_at_z5());
        let mut rng = Rng::new(3);
        let n = 600;
        let mut sum = 0.0;
        for _ in 0..n {
            let target = Vec3::new(rng.range(-0.6, 0.6), rng.range(-0.6, 0.6), -5.0);
            let ray = Ray::new(Vec3::ZERO, target.normalized());
            sum += radiance(&scene, ray, 550.0, &mut rng, 8);
        }
        let mean = sum / f64::from(n);
        assert!((mean - 0.5).abs() < 5e-3, "mean {mean}");
    }

    #[test]
    fn glass_sphere_in_a_furnace_conserves_energy() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.add(Material::Dielectric(&SF11), sphere_at_z5());
        let mut rng = Rng::new(11);
        for _ in 0..300 {
            let target = Vec3::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5), -5.0);
            let nm = rng.range(380.0, 780.0);
            let ray = Ray::new(Vec3::ZERO, target.normalized());
            let l = radiance(&scene, ray, nm, &mut rng, 64);
            assert!((l - 1.0).abs() < 1e-9, "wavelength {nm}: {l}");
        }
    }

    #[test]
    fn thin_film_sphere_in_a_furnace_conserves_energy() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        let film = Material::ThinFilm {
            index: 1.33,
            thickness_nm: 280.0,
        };
        scene.add(film, sphere_at_z5());
        let mut rng = Rng::new(21);
        for _ in 0..300 {
            let target = Vec3::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5), -5.0);
            let nm = rng.range(380.0, 780.0);
            let ray = Ray::new(Vec3::ZERO, target.normalized());
            let l = radiance(&scene, ray, nm, &mut rng, 64);
            assert!((l - 1.0).abs() < 1e-9, "wavelength {nm}: {l}");
        }
    }

    #[test]
    fn cosine_samples_stay_in_the_hemisphere() {
        let mut rng = Rng::new(5);
        let n = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..500 {
            let d = cosine_hemisphere(n, &mut rng);
            assert!(d.dot(n) >= -1e-12);
            assert!((d.length() - 1.0).abs() < 1e-12);
        }
    }
}
