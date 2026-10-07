//! Film: spectral-to-RGB conversion with white balance, and the image buffer.

use std::sync::OnceLock;

use crate::color::{cmf, to_srgb8, xyz_to_linear_srgb};
use crate::math::Vec3;

static WHITE_GAIN: OnceLock<Vec3> = OnceLock::new();

/// Per-channel gain that maps an equal-energy (flat) spectrum to pure white.
pub fn flat_white_gain() -> Vec3 {
    *WHITE_GAIN.get_or_init(|| {
        let sum = (380..=780).fold(Vec3::ZERO, |a, n| a + cmf(f64::from(n)));
        let rgb = xyz_to_linear_srgb(sum / sum.y);
        Vec3::new(1.0 / rgb.x, 1.0 / rgb.y, 1.0 / rgb.z)
    })
}

/// Converts accumulated XYZ to white-balanced linear sRGB.
pub fn film_rgb(xyz: Vec3) -> Vec3 {
    xyz_to_linear_srgb(xyz) * flat_white_gain()
}

/// A linear sRGB image.
#[derive(Clone, Debug)]
pub struct Image {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Row-major pixels, top row first.
    pub pixels: Vec<Vec3>,
}

impl Image {
    /// Creates a black image.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Vec3::ZERO; width * height],
        }
    }

    /// Gamma-encoded 8-bit RGB bytes, row-major.
    pub fn to_srgb8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for p in &self.pixels {
            out.extend_from_slice(&to_srgb8(*p));
        }
        out
    }

    /// Encodes the image as a PNG file in memory.
    pub fn to_png(&self) -> Vec<u8> {
        crate::png::encode_rgb8(self.width as u32, self.height as u32, &self.to_srgb8())
    }

    /// Average pixel value.
    pub fn mean(&self) -> Vec3 {
        if self.pixels.is_empty() {
            return Vec3::ZERO;
        }
        let sum = self.pixels.iter().fold(Vec3::ZERO, |a, p| a + *p);
        sum / self.pixels.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_spectrum_maps_to_white() {
        let sum = (380..=780).fold(Vec3::ZERO, |a, n| a + cmf(f64::from(n)));
        assert!(film_rgb(sum / sum.y).near(Vec3::splat(1.0), 1e-9));
        assert_eq!(film_rgb(Vec3::ZERO), Vec3::ZERO);
    }

    #[test]
    fn image_buffers() {
        let mut img = Image::new(2, 3);
        assert_eq!(img.pixels.len(), 6);
        assert_eq!(img.to_srgb8().len(), 18);
        img.pixels[0] = Vec3::splat(6.0);
        assert!(img.mean().near(Vec3::splat(1.0), 1e-12));
        assert_eq!(Image::new(0, 0).mean(), Vec3::ZERO);
    }

    #[test]
    fn png_bytes_start_with_the_signature() {
        let png = Image::new(4, 4).to_png();
        assert_eq!(&png[1..4], b"PNG");
    }
}
