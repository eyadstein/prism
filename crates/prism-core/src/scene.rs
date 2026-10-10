//! Scenes: a sky model plus one BVH per material.

use crate::atmosphere::Atmosphere;
use crate::bvh::Bvh;
use crate::geometry::{Hit, Primitive};
use crate::material::Material;
use crate::math::{Ray, Vec3};

/// Background illumination: a vertical gradient of white light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Radiance at and below the horizon.
    pub horizon: f64,
    /// Radiance straight up.
    pub zenith: f64,
}

impl Sky {
    /// A sky with the same radiance in every direction.
    pub const fn uniform(radiance: f64) -> Self {
        Self {
            horizon: radiance,
            zenith: radiance,
        }
    }

    /// Radiance seen along `dir`.
    pub fn radiance(&self, dir: Vec3) -> f64 {
        let t = dir.normalized().y.clamp(0.0, 1.0);
        self.horizon + (self.zenith - self.horizon) * t
    }
}

#[derive(Clone, Debug)]
struct Group {
    material: Material,
    bvh: Bvh,
}

/// Geometry grouped by material, lit by a [`Sky`] or, when set, an [`Atmosphere`].
#[derive(Clone, Debug)]
pub struct Scene {
    groups: Vec<Group>,
    /// Background illumination used when there is no atmosphere.
    pub sky: Sky,
    /// Physically based sky; when set it replaces `sky`.
    pub atmosphere: Option<Atmosphere>,
}

impl Scene {
    /// Creates an empty scene.
    pub fn new(sky: Sky) -> Self {
        Self {
            groups: Vec::new(),
            sky,
            atmosphere: None,
        }
    }

    /// Radiance of the background seen along `dir` at wavelength `nm`.
    pub fn background(&self, dir: Vec3, nm: f64) -> f64 {
        self.atmosphere
            .as_ref()
            .map_or_else(|| self.sky.radiance(dir), |a| a.radiance(dir, nm))
    }

    /// Adds many primitives that share one material and one BVH. Prefer this over
    /// repeated [`Scene::add`] calls for large meshes.
    pub fn add_group(&mut self, material: Material, prims: Vec<Primitive>) {
        if !prims.is_empty() {
            self.groups.push(Group {
                material,
                bvh: Bvh::build(prims),
            });
        }
    }

    /// Adds a single primitive.
    pub fn add(&mut self, material: Material, prim: Primitive) {
        self.add_group(material, vec![prim]);
    }

    /// Total number of primitives.
    pub fn primitive_count(&self) -> usize {
        self.groups.iter().map(|g| g.bvh.len()).sum()
    }

    /// Nearest intersection with `t_min < t < t_max`, together with the surface material.
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<(Hit, &Material)> {
        let mut closest = t_max;
        let mut best = None;
        for group in &self.groups {
            if let Some(hit) = group.bvh.intersect(ray, t_min, closest) {
                closest = hit.t;
                best = Some((hit, &group.material));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Sphere;

    #[test]
    fn sky_gradient() {
        let sky = Sky {
            horizon: 0.2,
            zenith: 1.0,
        };
        assert!((sky.radiance(Vec3::new(0.0, 1.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((sky.radiance(Vec3::new(1.0, 0.0, 0.0)) - 0.2).abs() < 1e-12);
        assert!((sky.radiance(Vec3::new(0.0, -1.0, 0.0)) - 0.2).abs() < 1e-12);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((sky.radiance(Vec3::new(0.0, 1.0, 1.0)) - (0.2 + 0.8 * s)).abs() < 1e-12);
    }

    #[test]
    fn nearest_group_wins() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        let far = Sphere::new(Vec3::new(0.0, 0.0, -10.0), 1.0);
        let near = Sphere::new(Vec3::new(0.0, 0.0, -4.0), 1.0);
        scene.add(
            Material::Diffuse { reflectance: 0.5 },
            Primitive::Sphere(far),
        );
        scene.add(
            Material::Mirror { reflectance: 0.9 },
            Primitive::Sphere(near),
        );
        assert_eq!(scene.primitive_count(), 2);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let (hit, material) = scene.intersect(&ray, 1e-6, f64::INFINITY).expect("hit");
        assert!((hit.t - 3.0).abs() < 1e-9);
        assert!(matches!(material, Material::Mirror { .. }));
    }

    #[test]
    fn empty_groups_are_ignored() {
        let mut scene = Scene::new(Sky::uniform(1.0));
        scene.add_group(Material::Diffuse { reflectance: 0.5 }, Vec::new());
        assert_eq!(scene.primitive_count(), 0);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        assert!(scene.intersect(&ray, 1e-6, f64::INFINITY).is_none());
    }

    #[test]
    fn new_scenes_have_no_atmosphere() {
        let scene = Scene::new(Sky::uniform(0.4));
        assert!(scene.atmosphere.is_none());
        let dir = Vec3::new(0.0, 1.0, 0.0);
        assert!((scene.background(dir, 550.0) - 0.4).abs() < 1e-12);
    }
}
