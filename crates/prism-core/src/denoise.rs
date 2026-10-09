//! Edge-avoiding a-trous wavelet denoiser (Dammertz et al., 2010).
//!
//! Each pass blurs with a 5 by 5 B3-spline kernel whose taps are `step` pixels apart
//! (1, 2, 4, ...), so a few passes cover a wide area cheaply. A neighbour counts for less
//! when its colour, albedo, normal or depth differs from the centre pixel, which keeps
//! object edges sharp while the noise is averaged away.

use crate::error::{PrismError, Result};
use crate::features::Features;
use crate::film::Image;
use crate::math::Vec3;

/// One-dimensional B3-spline weights indexed by the distance from the centre tap.
const KERNEL: [f64; 3] = [0.375, 0.25, 0.0625];
const MAX_ITERATIONS: u32 = 12;
const NO_DEPTH_MATCH: f64 = 1000.0;

/// How strongly each guide separates pixels. Smaller values preserve more detail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DenoiseSettings {
    /// Number of wavelet passes (0 to 12).
    pub iterations: u32,
    /// Colour tolerance of the first pass; it halves on every later pass.
    pub sigma_color: f64,
    /// Tolerance for the squared distance between surface normals.
    pub sigma_normal: f64,
    /// Tolerance for the relative difference in depth.
    pub sigma_depth: f64,
    /// Tolerance for the difference in albedo.
    pub sigma_albedo: f64,
}

impl Default for DenoiseSettings {
    fn default() -> Self {
        Self {
            iterations: 4,
            sigma_color: 0.5,
            sigma_normal: 0.3,
            sigma_depth: 0.05,
            sigma_albedo: 0.1,
        }
    }
}

fn validate(s: &DenoiseSettings) -> Result<()> {
    let sigmas = [
        ("sigma_color", s.sigma_color),
        ("sigma_normal", s.sigma_normal),
        ("sigma_depth", s.sigma_depth),
        ("sigma_albedo", s.sigma_albedo),
    ];
    for (name, value) in sigmas {
        if !(value.is_finite() && value > 0.0) {
            return Err(PrismError::InvalidParameter {
                name,
                reason: "must be a positive number".to_owned(),
            });
        }
    }
    if s.iterations > MAX_ITERATIONS {
        return Err(PrismError::InvalidParameter {
            name: "iterations",
            reason: format!("must be at most {MAX_ITERATIONS}"),
        });
    }
    Ok(())
}

/// Relative depth difference. Two sky pixels match; sky never matches a surface.
fn depth_distance(a: f64, b: f64) -> f64 {
    match (a.is_finite(), b.is_finite()) {
        (true, true) => (a - b).abs() / a.max(b).max(1e-9),
        (false, false) => 0.0,
        _ => NO_DEPTH_MATCH,
    }
}

/// Denoises `image` using the noise-free `features` of the same scene and size.
pub fn denoise(image: &Image, features: &Features, settings: &DenoiseSettings) -> Result<Image> {
    validate(settings)?;
    let (w, h) = (image.width, image.height);
    if features.width != w || features.height != h {
        return Err(PrismError::InvalidParameter {
            name: "features",
            reason: "size does not match the image".to_owned(),
        });
    }
    let inv_normal = 1.0 / (settings.sigma_normal * settings.sigma_normal);
    let inv_depth = 1.0 / (settings.sigma_depth * settings.sigma_depth);
    let inv_albedo = 1.0 / (settings.sigma_albedo * settings.sigma_albedo);

    let mut current = image.pixels.clone();
    let mut next = vec![Vec3::ZERO; current.len()];
    let mut sigma_color = settings.sigma_color;
    let mut step = 1_usize;
    for _ in 0..settings.iterations {
        let inv_color = 1.0 / (sigma_color * sigma_color);
        for y in 0..h {
            for x in 0..w {
                let p = y * w + x;
                let mut sum = Vec3::ZERO;
                let mut total = 0.0;
                for ky in 0..5_usize {
                    let Some(qy) = (y + ky * step).checked_sub(2 * step) else {
                        continue;
                    };
                    if qy >= h {
                        continue;
                    }
                    for kx in 0..5_usize {
                        let Some(qx) = (x + kx * step).checked_sub(2 * step) else {
                            continue;
                        };
                        if qx >= w {
                            continue;
                        }
                        let q = qy * w + qx;
                        let tap = KERNEL[ky.abs_diff(2)] * KERNEL[kx.abs_diff(2)];
                        let dc = (current[p] - current[q]).length_squared();
                        let dn = (features.normal[p] - features.normal[q]).length_squared();
                        let dd = depth_distance(features.depth[p], features.depth[q]);
                        let da = features.albedo[p] - features.albedo[q];
                        let penalty = dc * inv_color
                            + dn * inv_normal
                            + dd * dd * inv_depth
                            + da * da * inv_albedo;
                        let weight = tap * (-penalty).exp();
                        sum += current[q] * weight;
                        total += weight;
                    }
                }
                next[p] = sum / total;
            }
        }
        std::mem::swap(&mut current, &mut next);
        sigma_color *= 0.5;
        step *= 2;
    }
    Ok(Image {
        width: w,
        height: h,
        pixels: current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::compute;
    use crate::math::Rng;
    use crate::render::{render, RenderSettings};

    fn flat_features(w: usize, h: usize) -> Features {
        let n = w * h;
        Features {
            width: w,
            height: h,
            albedo: vec![0.5; n],
            normal: vec![Vec3::new(0.0, 0.0, 1.0); n],
            depth: vec![3.0; n],
        }
    }

    fn mse(a: &Image, b: &Image) -> f64 {
        let sum: f64 = a
            .pixels
            .iter()
            .zip(&b.pixels)
            .map(|(p, q)| (*p - *q).length_squared())
            .sum();
        sum / (3.0 * a.pixels.len() as f64)
    }

    #[test]
    fn constant_images_are_unchanged() {
        let mut image = Image::new(12, 9);
        for p in &mut image.pixels {
            *p = Vec3::new(0.3, 0.5, 0.7);
        }
        let out =
            denoise(&image, &flat_features(12, 9), &DenoiseSettings::default()).expect("denoises");
        for p in &out.pixels {
            assert!(p.near(Vec3::new(0.3, 0.5, 0.7), 1e-12), "{p:?}");
        }
    }

    #[test]
    fn white_noise_is_reduced() {
        let (w, h) = (32, 32);
        let mut truth = Image::new(w, h);
        let mut noisy = Image::new(w, h);
        let mut rng = Rng::new(7);
        for i in 0..w * h {
            truth.pixels[i] = Vec3::splat(0.5);
            noisy.pixels[i] = Vec3::new(
                0.5 + rng.range(-0.3, 0.3),
                0.5 + rng.range(-0.3, 0.3),
                0.5 + rng.range(-0.3, 0.3),
            );
        }
        let settings = DenoiseSettings {
            sigma_color: 1.0,
            ..DenoiseSettings::default()
        };
        let clean = denoise(&noisy, &flat_features(w, h), &settings).expect("denoises");
        let before = mse(&noisy, &truth);
        let after = mse(&clean, &truth);
        assert!(after < 0.2 * before, "before {before}, after {after}");
    }

    #[test]
    fn guides_keep_edges_sharp() {
        let (w, h) = (16, 8);
        let mut image = Image::new(w, h);
        let mut features = flat_features(w, h);
        for y in 0..h {
            for x in 0..w {
                let left = x < 8;
                image.pixels[y * w + x] = Vec3::splat(if left { 0.1 } else { 0.9 });
                features.albedo[y * w + x] = if left { 0.2 } else { 0.8 };
            }
        }
        let settings = DenoiseSettings {
            sigma_color: 1.0e6,
            ..DenoiseSettings::default()
        };
        let guided = denoise(&image, &features, &settings).expect("denoises");
        for y in 0..h {
            for x in 0..w {
                let expected = if x < 8 { 0.1 } else { 0.9 };
                let got = guided.pixels[y * w + x].x;
                assert!((got - expected).abs() < 1e-9, "({x}, {y}): {got}");
            }
        }
        let blurred = denoise(&image, &flat_features(w, h), &settings).expect("denoises");
        assert!(blurred.pixels[3 * w + 7].x > 0.3);
        assert!(blurred.pixels[3 * w + 8].x < 0.7);
    }

    #[test]
    fn zero_iterations_return_the_input() {
        let mut image = Image::new(6, 5);
        image.pixels[7] = Vec3::new(1.0, 2.0, 3.0);
        let settings = DenoiseSettings {
            iterations: 0,
            ..DenoiseSettings::default()
        };
        let out = denoise(&image, &flat_features(6, 5), &settings).expect("denoises");
        assert_eq!(out.pixels, image.pixels);
    }

    #[test]
    fn empty_images_are_fine() {
        let out = denoise(
            &Image::new(0, 0),
            &flat_features(0, 0),
            &DenoiseSettings::default(),
        )
        .expect("denoises");
        assert_eq!(out.pixels.len(), 0);
    }

    #[test]
    fn bad_input_is_rejected() {
        let image = Image::new(4, 4);
        let good = flat_features(4, 4);
        let defaults = DenoiseSettings::default();
        assert!(matches!(
            denoise(&image, &flat_features(5, 4), &defaults),
            Err(PrismError::InvalidParameter {
                name: "features",
                ..
            })
        ));
        let zero_depth = DenoiseSettings {
            sigma_depth: 0.0,
            ..defaults
        };
        assert!(matches!(
            denoise(&image, &good, &zero_depth),
            Err(PrismError::InvalidParameter {
                name: "sigma_depth",
                ..
            })
        ));
        let nan_color = DenoiseSettings {
            sigma_color: f64::NAN,
            ..defaults
        };
        assert!(matches!(
            denoise(&image, &good, &nan_color),
            Err(PrismError::InvalidParameter {
                name: "sigma_color",
                ..
            })
        ));
        let too_many = DenoiseSettings {
            iterations: 13,
            ..defaults
        };
        assert!(matches!(
            denoise(&image, &good, &too_many),
            Err(PrismError::InvalidParameter {
                name: "iterations",
                ..
            })
        ));
    }

    #[test]
    fn depth_distance_handles_sky() {
        assert!(depth_distance(f64::INFINITY, f64::INFINITY).abs() < 1e-15);
        assert!((depth_distance(f64::INFINITY, 3.0) - NO_DEPTH_MATCH).abs() < 1e-9);
        assert!((depth_distance(3.0, f64::INFINITY) - NO_DEPTH_MATCH).abs() < 1e-9);
        assert!(depth_distance(3.0, 3.0).abs() < 1e-15);
        assert!((depth_distance(2.0, 4.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn denoising_a_low_sample_render_moves_it_towards_the_reference() {
        let (scene, camera) = crate::demo::demo_scene(32.0 / 18.0);
        let low = RenderSettings {
            width: 32,
            height: 18,
            samples: 2,
            max_depth: 8,
            seed: 11,
        };
        let high = RenderSettings { samples: 64, ..low };
        let noisy = render(&scene, &camera, &low);
        let reference = render(&scene, &camera, &high);
        let features = compute(&scene, &camera, 32, 18);
        let clean = denoise(&noisy, &features, &DenoiseSettings::default()).expect("denoises");
        let before = mse(&noisy, &reference);
        let after = mse(&clean, &reference);
        assert!(after < before, "before {before}, after {after}");
    }
}
