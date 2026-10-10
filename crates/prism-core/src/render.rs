//! Multi-threaded progressive-quality renderer.

use crate::camera::RaySource;
use crate::color::{sample_wavelength, PixelAccumulator};
use crate::film::{film_rgb, Image};
use crate::integrator::radiance;
use crate::math::{Rng, Vec3};
use crate::scene::Scene;

/// Image size and sampling quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSettings {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// Spectral samples per pixel (at least 1).
    pub samples: u32,
    /// Maximum number of bounces per path.
    pub max_depth: u32,
    /// Seed that makes renders reproducible.
    pub seed: u64,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 320,
            height: 180,
            samples: 16,
            max_depth: 16,
            seed: 1,
        }
    }
}

fn pixel_rng(seed: u64, index: usize) -> Rng {
    let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    rng.next_u64();
    rng
}

fn render_pixel<C: RaySource>(
    scene: &Scene,
    camera: &C,
    settings: &RenderSettings,
    x: usize,
    y: usize,
) -> Vec3 {
    let mut rng = pixel_rng(settings.seed, y * settings.width + x);
    let samples = settings.samples.max(1);
    let spp = f64::from(samples);
    let mut acc = PixelAccumulator::new();
    for s in 0..samples {
        let (nm, pdf) = sample_wavelength((f64::from(s) + rng.next_f64()) / spp);
        let px = (x as f64 + rng.next_f64()) / settings.width as f64;
        let py = 1.0 - (y as f64 + rng.next_f64()) / settings.height as f64;
        let l = camera.sample(px, py, nm, &mut rng).map_or(0.0, |c| {
            c.weight * radiance(scene, c.ray, nm, &mut rng, settings.max_depth)
        });
        acc.add(nm, pdf, l);
    }
    film_rgb(acc.resolve())
}

/// Worker threads to use: every core natively, one on WebAssembly (which cannot spawn threads).
fn default_threads() -> usize {
    if cfg!(target_arch = "wasm32") {
        1
    } else {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    }
}

/// Renders `scene` through `camera` using every available core.
/// The result does not depend on the thread count.
pub fn render<C: RaySource>(scene: &Scene, camera: &C, settings: &RenderSettings) -> Image {
    render_with_threads(scene, camera, settings, default_threads())
}

/// Renders with an explicit number of worker threads. One thread renders on the calling thread.
pub fn render_with_threads<C: RaySource>(
    scene: &Scene,
    camera: &C,
    settings: &RenderSettings,
    threads: usize,
) -> Image {
    let (w, h) = (settings.width, settings.height);
    let mut image = Image::new(w, h);
    if w == 0 || h == 0 {
        return image;
    }
    let threads = threads.max(1);
    if threads == 1 {
        for (index, px) in image.pixels.iter_mut().enumerate() {
            *px = render_pixel(scene, camera, settings, index % w, index / w);
        }
        return image;
    }
    let band_rows = h.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (band, chunk) in image.pixels.chunks_mut(band_rows * w).enumerate() {
            scope.spawn(move || {
                let first = band * band_rows * w;
                for (i, px) in chunk.iter_mut().enumerate() {
                    let index = first + i;
                    *px = render_pixel(scene, camera, settings, index % w, index / w);
                }
            });
        }
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, CameraSample, RaySource};
    use crate::scene::Sky;

    fn camera() -> Camera {
        Camera::new(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
            60.0,
            1.0,
        )
    }

    struct Blocked;

    impl RaySource for Blocked {
        fn sample(&self, _s: f64, _t: f64, _nm: f64, _rng: &mut Rng) -> Option<CameraSample> {
            None
        }
    }

    struct Dim(Camera);

    impl RaySource for Dim {
        fn sample(&self, s: f64, t: f64, nm: f64, rng: &mut Rng) -> Option<CameraSample> {
            self.0
                .sample(s, t, nm, rng)
                .map(|c| CameraSample { weight: 0.5, ..c })
        }
    }

    #[test]
    fn empty_scene_renders_uniform_white() {
        let scene = Scene::new(Sky::uniform(1.0));
        let settings = RenderSettings {
            width: 8,
            height: 8,
            samples: 128,
            max_depth: 4,
            seed: 3,
        };
        let img = render(&scene, &camera(), &settings);
        assert_eq!(img.pixels.len(), 64);
        for p in &img.pixels {
            assert!(p.near(Vec3::splat(1.0), 0.15), "{p:?}");
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        let scene = Scene::new(Sky {
            horizon: 1.0,
            zenith: 0.3,
        });
        let settings = RenderSettings {
            width: 9,
            height: 7,
            samples: 4,
            max_depth: 4,
            seed: 99,
        };
        let a = render(&scene, &camera(), &settings);
        let b = render(&scene, &camera(), &settings);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn thread_count_does_not_change_the_image() {
        let scene = Scene::new(Sky {
            horizon: 1.0,
            zenith: 0.3,
        });
        let settings = RenderSettings {
            width: 9,
            height: 7,
            samples: 4,
            max_depth: 4,
            seed: 5,
        };
        let one = render_with_threads(&scene, &camera(), &settings, 1);
        let three = render_with_threads(&scene, &camera(), &settings, 3);
        let zero = render_with_threads(&scene, &camera(), &settings, 0);
        assert_eq!(one.pixels, three.pixels);
        assert_eq!(one.pixels, zero.pixels);
    }

    #[test]
    fn zero_sized_images_are_empty() {
        let scene = Scene::new(Sky::uniform(1.0));
        let settings = RenderSettings {
            width: 0,
            height: 5,
            ..RenderSettings::default()
        };
        assert_eq!(render(&scene, &camera(), &settings).pixels.len(), 0);
    }

    #[test]
    fn blocked_rays_render_black() {
        let scene = Scene::new(Sky::uniform(1.0));
        let settings = RenderSettings {
            width: 4,
            height: 4,
            samples: 4,
            max_depth: 4,
            seed: 1,
        };
        let img = render(&scene, &Blocked, &settings);
        assert!(img.pixels.iter().all(|p| *p == Vec3::ZERO));
    }

    #[test]
    fn sample_weights_scale_the_image() {
        let scene = Scene::new(Sky::uniform(1.0));
        let settings = RenderSettings {
            width: 6,
            height: 6,
            samples: 128,
            max_depth: 4,
            seed: 2,
        };
        let img = render(&scene, &Dim(camera()), &settings);
        for p in &img.pixels {
            assert!(p.near(Vec3::splat(0.5), 0.1), "{p:?}");
        }
    }
}
