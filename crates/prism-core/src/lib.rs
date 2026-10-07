//! Prism core: spectral light transport and lens design.

#![forbid(unsafe_code)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::return_self_not_must_use,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::unreadable_literal
)]

pub mod analysis;
pub mod bvh;
pub mod camera;
pub mod color;
pub mod demo;
pub mod error;
pub mod film;
pub mod geometry;
pub mod glass;
pub mod integrator;
pub mod lens;
pub mod material;
pub mod math;
pub mod obj;
pub mod optimize;
pub mod png;
pub mod render;
pub mod scene;
pub mod scenefile;

pub use bvh::Bvh;
pub use camera::Camera;
pub use error::{PrismError, Result};
pub use film::Image;
pub use geometry::{Hit, Primitive, Sphere, Triangle};
pub use math::{Aabb, Ray, Rng, Vec3};
pub use render::RenderSettings;
pub use scene::{Scene, Sky};
pub use scenefile::{CameraSpec, ImageSpec, SceneFile};

/// Crate version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shortest wavelength the engine samples, in nanometres.
pub const WAVELENGTH_MIN_NM: f64 = 380.0;

/// Longest wavelength the engine samples, in nanometres.
pub const WAVELENGTH_MAX_NM: f64 = 780.0;

/// Validates that `nm` lies inside the supported spectral range.
pub fn check_wavelength(nm: f64) -> Result<f64> {
    if (WAVELENGTH_MIN_NM..=WAVELENGTH_MAX_NM).contains(&nm) {
        Ok(nm)
    } else {
        Err(PrismError::WavelengthOutOfRange(nm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_visible_wavelengths() {
        assert!(check_wavelength(550.0).is_ok());
        assert!(check_wavelength(WAVELENGTH_MIN_NM).is_ok());
        assert!(check_wavelength(WAVELENGTH_MAX_NM).is_ok());
    }

    #[test]
    fn rejects_out_of_range_wavelengths() {
        assert_eq!(
            check_wavelength(100.0),
            Err(PrismError::WavelengthOutOfRange(100.0))
        );
        assert!(check_wavelength(f64::NAN).is_err());
    }

    #[test]
    fn version_is_set() {
        assert_ne!(VERSION, "");
    }
}
