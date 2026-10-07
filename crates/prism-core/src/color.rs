//! Spectral colour: CIE matching functions, XYZ accumulation, and sRGB output.
//!
//! The matching functions use the multi-lobe Gaussian fit of Wyman, Sloan and
//! Shirley (2013), which is accurate to within a percent or two of the CIE 1931
//! tables and needs no lookup data.

use std::sync::OnceLock;

use crate::math::Vec3;
use crate::{WAVELENGTH_MAX_NM, WAVELENGTH_MIN_NM};

fn lobe(nm: f64, mu: f64, sigma_lo: f64, sigma_hi: f64) -> f64 {
    let s = if nm < mu { sigma_lo } else { sigma_hi };
    let t = (nm - mu) / s;
    (-0.5 * t * t).exp()
}

/// CIE 1931 colour matching functions `(x, y, z)` at `nm` nanometres.
pub fn cmf(nm: f64) -> Vec3 {
    let x = 1.056 * lobe(nm, 599.8, 37.9, 31.0) + 0.362 * lobe(nm, 442.0, 16.0, 26.7)
        - 0.065 * lobe(nm, 501.1, 20.4, 26.2);
    let y = 0.821 * lobe(nm, 568.8, 46.9, 40.5) + 0.286 * lobe(nm, 530.9, 16.3, 31.1);
    let z = 1.217 * lobe(nm, 437.0, 11.8, 36.0) + 0.681 * lobe(nm, 459.0, 26.0, 13.8);
    Vec3::new(x, y, z)
}

static Y_INTEGRAL: OnceLock<f64> = OnceLock::new();

/// Integral of the `y` matching function over the supported range (1 nm steps).
pub fn y_integral() -> f64 {
    *Y_INTEGRAL.get_or_init(|| (380..=780).map(|n| cmf(f64::from(n)).y).sum())
}

/// Converts CIE XYZ to linear (not gamma encoded) sRGB primaries.
pub fn xyz_to_linear_srgb(xyz: Vec3) -> Vec3 {
    Vec3::new(
        3.240_454_2 * xyz.x - 1.537_138_5 * xyz.y - 0.498_531_4 * xyz.z,
        -0.969_266_0 * xyz.x + 1.876_010_8 * xyz.y + 0.041_556_0 * xyz.z,
        0.055_643_4 * xyz.x - 0.204_025_9 * xyz.y + 1.057_225_2 * xyz.z,
    )
}

/// sRGB transfer function for one linear channel in `[0, 1]`.
pub fn srgb_encode(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Gamma encodes linear sRGB and quantizes to 8 bits per channel, clamping out-of-gamut values.
pub fn to_srgb8(linear: Vec3) -> [u8; 3] {
    let q = |c: f64| (srgb_encode(c.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
    [q(linear.x), q(linear.y), q(linear.z)]
}

/// Uniformly samples a wavelength from `u` in `[0, 1)`. Returns `(nm, pdf)`.
pub fn sample_wavelength(u: f64) -> (f64, f64) {
    let span = WAVELENGTH_MAX_NM - WAVELENGTH_MIN_NM;
    (WAVELENGTH_MIN_NM + u * span, 1.0 / span)
}

/// Accumulates spectral radiance samples for one pixel.
#[derive(Clone, Debug, Default)]
pub struct PixelAccumulator {
    xyz: Vec3,
    samples: u32,
}

impl PixelAccumulator {
    /// Creates an empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a radiance sample taken at `nm` with sampling density `pdf`.
    pub fn add(&mut self, nm: f64, pdf: f64, radiance: f64) {
        self.xyz += cmf(nm) * (radiance / pdf);
        self.samples += 1;
    }

    /// Number of samples added so far.
    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// Mean XYZ, normalized so a flat spectrum of radiance 1 gives `Y = 1`.
    pub fn resolve(&self) -> Vec3 {
        if self.samples == 0 {
            Vec3::ZERO
        } else {
            self.xyz / (f64::from(self.samples) * y_integral())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_peaks_near_555() {
        let y = cmf(555.0).y;
        assert!((0.98..=1.02).contains(&y), "y(555) = {y}");
        assert!(cmf(380.0).y < 0.01);
        assert!(cmf(780.0).y < 0.01);
    }

    #[test]
    fn flat_spectrum_is_nearly_neutral() {
        let sum = (380..=780).fold(Vec3::ZERO, |a, n| a + cmf(f64::from(n)));
        let total = sum.x + sum.y + sum.z;
        assert!((sum.x / total - 1.0 / 3.0).abs() < 0.02);
        assert!((sum.y / total - 1.0 / 3.0).abs() < 0.02);
    }

    #[test]
    fn monochromatic_hues() {
        let red = xyz_to_linear_srgb(cmf(650.0));
        assert!(red.x > red.y && red.x > red.z);
        let green = xyz_to_linear_srgb(cmf(530.0));
        assert!(green.y > green.x && green.y > green.z);
        let blue = xyz_to_linear_srgb(cmf(450.0));
        assert!(blue.z > blue.x && blue.z > blue.y);
    }

    #[test]
    fn srgb_transfer_function() {
        assert!(srgb_encode(0.0).abs() < 1e-12);
        assert!((srgb_encode(1.0) - 1.0).abs() < 1e-12);
        assert!((srgb_encode(0.5) - 0.7354).abs() < 1e-3);
        assert_eq!(to_srgb8(Vec3::new(2.0, -1.0, 0.0)), [255, 0, 0]);
    }

    #[test]
    fn wavelength_sampling_covers_range() {
        let (lo, pdf) = sample_wavelength(0.0);
        let (hi, _) = sample_wavelength(0.999_999);
        assert!((lo - 380.0).abs() < 1e-9);
        assert!(hi < 780.0 && hi > 779.9);
        assert!((pdf - 1.0 / 400.0).abs() < 1e-12);
    }

    #[test]
    fn accumulator_normalizes_flat_white() {
        let mut acc = PixelAccumulator::new();
        assert_eq!(acc.resolve(), Vec3::ZERO);
        let n = 4000;
        for i in 0..n {
            let (nm, pdf) = sample_wavelength((f64::from(i) + 0.5) / f64::from(n));
            acc.add(nm, pdf, 1.0);
        }
        assert_eq!(acc.samples(), 4000);
        assert!((acc.resolve().y - 1.0).abs() < 1e-3);
    }
}
