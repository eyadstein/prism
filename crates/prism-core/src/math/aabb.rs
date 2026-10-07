//! Axis-aligned bounding boxes.

use super::{Ray, Vec3};

/// An axis-aligned box. The empty box has `min > max` on every axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    /// Lower corner.
    pub min: Vec3,
    /// Upper corner.
    pub max: Vec3,
}

impl Aabb {
    /// The box containing nothing; the identity for [`Aabb::union`].
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f64::INFINITY),
        max: Vec3::splat(f64::NEG_INFINITY),
    };

    /// Smallest box containing both points.
    pub fn from_points(a: Vec3, b: Vec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Smallest box containing `self` and `o`.
    pub fn union(self, o: Self) -> Self {
        Self {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
        }
    }

    /// Smallest box containing `self` and the point `p`.
    pub fn grow(self, p: Vec3) -> Self {
        Self {
            min: self.min.min(p),
            max: self.max.max(p),
        }
    }

    /// Centre of the box.
    pub fn centroid(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Size along each axis.
    pub fn extent(&self) -> Vec3 {
        self.max - self.min
    }

    /// Total surface area, or zero for the empty box.
    pub fn surface_area(&self) -> f64 {
        if self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z {
            return 0.0;
        }
        let e = self.extent();
        2.0 * (e.x * e.y + e.y * e.z + e.z * e.x)
    }

    /// Index (0, 1, 2) of the longest axis.
    pub fn longest_axis(&self) -> usize {
        let e = self.extent();
        if e.x >= e.y && e.x >= e.z {
            0
        } else if e.y >= e.z {
            1
        } else {
            2
        }
    }

    /// Slab test. `inv_dir` is the component-wise reciprocal of the ray direction.
    pub fn hit(&self, ray: &Ray, inv_dir: Vec3, t_max: f64) -> bool {
        let mut t0 = 0.0_f64;
        let mut t1 = t_max;
        for axis in 0..3 {
            let inv = inv_dir.component(axis);
            let a = (self.min.component(axis) - ray.origin.component(axis)) * inv;
            let b = (self.max.component(axis) - ray.origin.component(axis)) * inv;
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b) * 1.000_000_000_000_001);
            if t0 > t1 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Aabb {
        Aabb::from_points(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0))
    }

    fn inv(d: Vec3) -> Vec3 {
        Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z)
    }

    #[test]
    fn ray_hits_and_misses() {
        let b = unit_box();
        let d = Vec3::new(0.0, 0.0, -1.0);
        let hit = Ray::new(Vec3::new(0.0, 0.0, 5.0), d);
        let miss = Ray::new(Vec3::new(3.0, 0.0, 5.0), d);
        assert!(b.hit(&hit, inv(d), f64::INFINITY));
        assert!(!b.hit(&miss, inv(d), f64::INFINITY));
        assert!(!b.hit(&hit, inv(d), 1.0));
    }

    #[test]
    fn union_with_empty_is_identity() {
        let b = unit_box();
        assert_eq!(Aabb::EMPTY.union(b), b);
        assert_eq!(b.union(Aabb::EMPTY), b);
        assert!(Aabb::EMPTY.surface_area().abs() < f64::EPSILON);
    }

    #[test]
    fn area_and_axis() {
        let b = Aabb::from_points(Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0));
        assert!((b.surface_area() - 22.0).abs() < 1e-12);
        assert_eq!(b.longest_axis(), 2);
        assert!(b.centroid().near(Vec3::new(0.5, 1.0, 1.5), 1e-12));
    }
}
