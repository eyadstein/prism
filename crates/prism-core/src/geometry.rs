//! Ray-intersectable primitives.

use crate::math::{Aabb, Ray, Vec3};

/// Result of a successful ray intersection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Ray parameter at the hit.
    pub t: f64,
    /// World-space hit point.
    pub point: Vec3,
    /// Unit surface normal, flipped to face against the ray.
    pub normal: Vec3,
    /// True when the ray struck the outside (geometric normal faces the ray).
    pub front_face: bool,
}

/// A sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    /// Centre.
    pub center: Vec3,
    /// Radius.
    pub radius: f64,
}

impl Sphere {
    /// Creates a sphere.
    pub const fn new(center: Vec3, radius: f64) -> Self {
        Self { center, radius }
    }

    /// Tight bounding box.
    pub fn bounds(&self) -> Aabb {
        let r = Vec3::splat(self.radius);
        Aabb {
            min: self.center - r,
            max: self.center + r,
        }
    }

    /// Nearest intersection with `t_min < t < t_max`.
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<Hit> {
        let oc = ray.origin - self.center;
        let a = ray.dir.length_squared();
        let half_b = oc.dot(ray.dir);
        let c = oc.length_squared() - self.radius * self.radius;
        let disc = half_b * half_b - a * c;
        if disc < 0.0 || a == 0.0 {
            return None;
        }
        let sq = disc.sqrt();
        let mut t = (-half_b - sq) / a;
        if !(t > t_min && t < t_max) {
            t = (-half_b + sq) / a;
            if !(t > t_min && t < t_max) {
                return None;
            }
        }
        let point = ray.at(t);
        let outward = (point - self.center) / self.radius;
        let front_face = ray.dir.dot(outward) < 0.0;
        Some(Hit {
            t,
            point,
            normal: if front_face { outward } else { -outward },
            front_face,
        })
    }
}

/// A triangle (Moller-Trumbore intersection, double sided).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// First vertex.
    pub a: Vec3,
    /// Second vertex.
    pub b: Vec3,
    /// Third vertex.
    pub c: Vec3,
}

impl Triangle {
    /// Creates a triangle.
    pub const fn new(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self { a, b, c }
    }

    /// Tight bounding box.
    pub fn bounds(&self) -> Aabb {
        Aabb::EMPTY.grow(self.a).grow(self.b).grow(self.c)
    }

    /// Nearest intersection with `t_min < t < t_max`.
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<Hit> {
        let e1 = self.b - self.a;
        let e2 = self.c - self.a;
        let p = ray.dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-14 {
            return None;
        }
        let inv_det = 1.0 / det;
        let tv = ray.origin - self.a;
        let u = tv.dot(p) * inv_det;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = tv.cross(e1);
        let v = ray.dir.dot(q) * inv_det;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = e2.dot(q) * inv_det;
        if !(t > t_min && t < t_max) {
            return None;
        }
        let geometric = e1.cross(e2).normalized();
        let front_face = geometric.dot(ray.dir) < 0.0;
        Some(Hit {
            t,
            point: ray.at(t),
            normal: if front_face { geometric } else { -geometric },
            front_face,
        })
    }
}

/// Any shape the BVH can hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    /// A sphere.
    Sphere(Sphere),
    /// A triangle.
    Triangle(Triangle),
}

impl Primitive {
    /// Tight bounding box.
    pub fn bounds(&self) -> Aabb {
        match self {
            Self::Sphere(s) => s.bounds(),
            Self::Triangle(t) => t.bounds(),
        }
    }

    /// Nearest intersection with `t_min < t < t_max`.
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<Hit> {
        match self {
            Self::Sphere(s) => s.intersect(ray, t_min, t_max),
            Self::Triangle(t) => t.intersect(ray, t_min, t_max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward() -> Ray {
        Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0))
    }

    #[test]
    fn sphere_front_hit() {
        let s = Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0);
        let h = s.intersect(&forward(), 1e-9, f64::INFINITY).expect("hit");
        assert!((h.t - 4.0).abs() < 1e-12);
        assert!(h.front_face);
        assert!(h.normal.near(Vec3::new(0.0, 0.0, 1.0), 1e-12));
    }

    #[test]
    fn sphere_inside_hit_is_back_face() {
        let s = Sphere::new(Vec3::ZERO, 1.0);
        let h = s.intersect(&forward(), 1e-9, f64::INFINITY).expect("hit");
        assert!((h.t - 1.0).abs() < 1e-12);
        assert!(!h.front_face);
        assert!(h.normal.near(Vec3::new(0.0, 0.0, 1.0), 1e-12));
    }

    #[test]
    fn sphere_miss_and_range() {
        let s = Sphere::new(Vec3::new(5.0, 0.0, -5.0), 1.0);
        assert!(s.intersect(&forward(), 1e-9, f64::INFINITY).is_none());
        let s2 = Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0);
        assert!(s2.intersect(&forward(), 1e-9, 3.0).is_none());
    }

    fn tri() -> Triangle {
        Triangle::new(
            Vec3::new(-1.0, -1.0, -2.0),
            Vec3::new(1.0, -1.0, -2.0),
            Vec3::new(0.0, 1.0, -2.0),
        )
    }

    #[test]
    fn triangle_hit_and_miss() {
        let h = tri()
            .intersect(&forward(), 1e-9, f64::INFINITY)
            .expect("hit");
        assert!((h.t - 2.0).abs() < 1e-12);
        let off = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
        assert!(tri().intersect(&off, 1e-9, f64::INFINITY).is_none());
    }

    #[test]
    fn triangle_is_double_sided() {
        let back = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0));
        let h = tri().intersect(&back, 1e-9, f64::INFINITY).expect("hit");
        assert!((h.t - 3.0).abs() < 1e-12);
        assert!(h.normal.dot(back.dir) < 0.0);
    }

    #[test]
    fn primitive_dispatches() {
        let p = Primitive::Sphere(Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0));
        assert!(p.intersect(&forward(), 1e-9, f64::INFINITY).is_some());
        assert!(p.bounds().surface_area() > 0.0);
    }
}
