//! Rays.

use super::Vec3;

/// A half-line `origin + t * dir` for `t >= 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Starting point.
    pub origin: Vec3,
    /// Direction; not required to be normalized.
    pub dir: Vec3,
}

impl Ray {
    /// Creates a ray.
    pub const fn new(origin: Vec3, dir: Vec3) -> Self {
        Self { origin, dir }
    }

    /// Point at parameter `t`.
    pub fn at(&self, t: f64) -> Vec3 {
        self.origin + self.dir * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_walks_along_direction() {
        let r = Ray::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(r.at(1.5), Vec3::new(1.0, 3.0, 0.0));
    }
}
