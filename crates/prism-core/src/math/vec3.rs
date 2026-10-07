//! Three-component double precision vector.

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// A point, direction, or colour triple in 3D space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Self = Self::splat(0.0);

    /// Builds a vector from components.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Builds a vector with all components equal to `v`.
    pub const fn splat(v: f64) -> Self {
        Self { x: v, y: v, z: v }
    }

    /// Dot product.
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Cross product.
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Squared Euclidean length.
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// Euclidean length.
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Unit-length copy, or the input unchanged when it has zero length.
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 0.0 {
            self / l
        } else {
            self
        }
    }

    /// Mirror reflection about unit normal `n`.
    pub fn reflect(self, n: Self) -> Self {
        self - n * (2.0 * self.dot(n))
    }

    /// Refraction of a unit incident direction through a surface with unit normal `n`
    /// facing against the incident ray. `eta` is `n1 / n2`. Returns `None` on total
    /// internal reflection.
    pub fn refract(self, n: Self, eta: f64) -> Option<Self> {
        let cos_i = -self.dot(n);
        let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
        if sin2_t > 1.0 {
            None
        } else {
            let cos_t = (1.0 - sin2_t).sqrt();
            Some(self * eta + n * (eta * cos_i - cos_t))
        }
    }

    /// Component-wise minimum.
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    /// Component-wise maximum.
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    /// Component by axis index (0 = x, 1 = y, anything else = z).
    pub fn component(self, axis: usize) -> f64 {
        match axis {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    /// True when every component differs from `o` by less than `eps`.
    pub fn near(self, o: Self, eps: f64) -> bool {
        (self.x - o.x).abs() < eps && (self.y - o.y).abs() < eps && (self.z - o.z).abs() < eps
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}

impl Mul for Vec3 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.x / s, self.y / s, self.z / s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn dot_and_cross() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(-2.0, 0.5, 4.0);
        assert!(close(a.dot(b), 11.0));
        let c = a.cross(b);
        assert!(close(c.dot(a), 0.0));
        assert!(close(c.dot(b), 0.0));
    }

    #[test]
    fn normalization() {
        let v = Vec3::new(3.0, 4.0, 12.0).normalized();
        assert!(close(v.length(), 1.0));
        assert_eq!(Vec3::ZERO.normalized(), Vec3::ZERO);
    }

    #[test]
    fn reflection_flips_normal_component() {
        let n = Vec3::new(0.0, 1.0, 0.0);
        let r = Vec3::new(1.0, -1.0, 0.0).reflect(n);
        assert!(r.near(Vec3::new(1.0, 1.0, 0.0), 1e-12));
    }

    #[test]
    fn refraction_obeys_snells_law() {
        let sin_i = 0.5_f64;
        let cos_i = (1.0 - sin_i * sin_i).sqrt();
        let incident = Vec3::new(sin_i, 0.0, -cos_i);
        let n = Vec3::new(0.0, 0.0, 1.0);
        let eta = 1.0 / 1.5;
        let t = incident
            .refract(n, eta)
            .expect("no total internal reflection");
        assert!(close(t.length(), 1.0));
        assert!(close(t.x, eta * sin_i));
    }

    #[test]
    fn total_internal_reflection_returns_none() {
        let sin_i = 0.9_f64;
        let incident = Vec3::new(sin_i, 0.0, -(1.0 - sin_i * sin_i).sqrt());
        assert!(incident.refract(Vec3::new(0.0, 0.0, 1.0), 1.5).is_none());
    }

    #[test]
    fn operators_work() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(a + a, a * 2.0);
        assert_eq!(2.0 * a, a * 2.0);
        assert_eq!(a - a, Vec3::ZERO);
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(a / 2.0, Vec3::new(0.5, 1.0, 1.5));
    }
}
