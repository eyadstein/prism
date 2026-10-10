//! Sequential lens systems: prescriptions, paraxial analysis, real ray tracing, spot diagrams.
//!
//! Light travels along +z. Surface `i` has its vertex on the axis at the sum of the
//! thicknesses of all earlier surfaces. The last thickness is the distance to the image plane.

use crate::error::{PrismError, Result};
use crate::glass::Glass;
use crate::math::{Ray, Vec3};

/// Wavelength of the sodium d line, the conventional reference for focal lengths.
pub const D_LINE_NM: f64 = 587.56;

/// One refracting surface together with the medium behind it.
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    /// Radius of curvature, positive when the centre lies ahead of the vertex. Zero (or
    /// infinite) means a flat surface.
    pub radius: f64,
    /// Axial distance to the next surface, or to the image plane for the last surface.
    pub thickness: f64,
    /// Medium behind the surface; `None` means air.
    pub glass: Option<&'static Glass>,
    /// Largest ray height that passes this surface.
    pub semi_aperture: f64,
}

impl Surface {
    /// Curvature `1 / radius`, or zero for a flat surface.
    pub fn curvature(&self) -> f64 {
        if self.radius == 0.0 {
            0.0
        } else {
            1.0 / self.radius
        }
    }

    /// Refractive index of the medium behind the surface at `nm` nanometres.
    pub fn index(&self, nm: f64) -> f64 {
        self.glass.map_or(1.0, |g| g.index(nm))
    }

    /// Intersection point and a unit normal facing against the ray.
    fn intersect(&self, ray: &Ray, vertex_z: f64) -> Option<(Vec3, Vec3)> {
        let c = self.curvature();
        let (t, outward) = if c == 0.0 {
            if ray.dir.z == 0.0 {
                return None;
            }
            (
                (vertex_z - ray.origin.z) / ray.dir.z,
                Vec3::new(0.0, 0.0, -1.0),
            )
        } else {
            let radius = 1.0 / c;
            let center = Vec3::new(0.0, 0.0, vertex_z + radius);
            let oc = ray.origin - center;
            let a = ray.dir.length_squared();
            let half_b = oc.dot(ray.dir);
            let disc = half_b * half_b - a * (oc.length_squared() - radius * radius);
            if disc < 0.0 {
                return None;
            }
            let sq = disc.sqrt();
            let root = if radius > 0.0 {
                -half_b - sq
            } else {
                -half_b + sq
            };
            let t = root / a;
            (t, (ray.at(t) - center).normalized())
        };
        let point = ray.at(t);
        let normal = if outward.dot(ray.dir) > 0.0 {
            -outward
        } else {
            outward
        };
        Some((point, normal))
    }
}

/// Why a ray did not make it through the lens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RayFate {
    /// The ray never met a surface.
    Missed,
    /// The ray hit a surface outside its semi-aperture.
    Vignetted,
    /// The ray could not refract.
    TotalInternalReflection,
}

/// First-order (paraxial) properties of a lens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paraxial {
    /// Effective focal length.
    pub efl: f64,
    /// Distance from the last vertex to the paraxial focus.
    pub bfd: f64,
}

/// Where traced rays landed on the image plane.
#[derive(Clone, Debug)]
pub struct Spot {
    /// Image-plane `(x, y)` positions of the rays that got through.
    pub points: Vec<(f64, f64)>,
    /// Number of rays launched.
    pub launched: usize,
}

impl Spot {
    /// Mean position of the surviving rays.
    pub fn centroid(&self) -> Option<(f64, f64)> {
        if self.points.is_empty() {
            return None;
        }
        let n = self.points.len() as f64;
        let (sx, sy) = self
            .points
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        Some((sx / n, sy / n))
    }

    /// Root-mean-square distance of the rays from their centroid.
    pub fn rms_radius(&self) -> Option<f64> {
        let (cx, cy) = self.centroid()?;
        let n = self.points.len() as f64;
        let sum: f64 = self
            .points
            .iter()
            .map(|p| (p.0 - cx).powi(2) + (p.1 - cy).powi(2))
            .sum();
        Some((sum / n).sqrt())
    }

    /// Fraction of launched rays that reached the image plane.
    pub fn transmitted_fraction(&self) -> f64 {
        if self.launched == 0 {
            0.0
        } else {
            self.points.len() as f64 / self.launched as f64
        }
    }
}

/// An ordered list of surfaces.
#[derive(Clone, Debug)]
pub struct Lens {
    surfaces: Vec<Surface>,
}

impl Lens {
    /// Validates and builds a lens.
    pub fn new(surfaces: Vec<Surface>) -> Result<Self> {
        if surfaces.is_empty() {
            return Err(PrismError::InvalidParameter {
                name: "surfaces",
                reason: "a lens needs at least one surface".to_owned(),
            });
        }
        for s in &surfaces {
            if !s.thickness.is_finite() || s.thickness < 0.0 {
                return Err(PrismError::InvalidParameter {
                    name: "thickness",
                    reason: "must be finite and not negative".to_owned(),
                });
            }
            if s.radius.is_nan() {
                return Err(PrismError::InvalidParameter {
                    name: "radius",
                    reason: "must be a number".to_owned(),
                });
            }
            if s.semi_aperture.is_nan() || s.semi_aperture <= 0.0 {
                return Err(PrismError::InvalidParameter {
                    name: "semi_aperture",
                    reason: "must be positive".to_owned(),
                });
            }
        }
        Ok(Self { surfaces })
    }

    /// Parses a prescription: one surface per line as `radius thickness glass [aperture]`.
    /// `flat` means a plane, `air` means no glass, `#` starts a comment.
    pub fn parse(text: &str) -> Result<Self> {
        let mut surfaces = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let number = index + 1;
            let fail = |reason: String| PrismError::Parse {
                line: number,
                reason,
            };
            let cols: Vec<&str> = line.split_whitespace().collect();
            if !(3..=4).contains(&cols.len()) {
                return Err(fail(format!(
                    "expected `radius thickness glass [aperture]`, found {} columns",
                    cols.len()
                )));
            }
            let radius = if cols[0].eq_ignore_ascii_case("flat") {
                0.0
            } else {
                cols[0]
                    .parse::<f64>()
                    .map_err(|_| fail(format!("bad radius `{}`", cols[0])))?
            };
            let thickness = cols[1]
                .parse::<f64>()
                .map_err(|_| fail(format!("bad thickness `{}`", cols[1])))?;
            let glass = if cols[2].eq_ignore_ascii_case("air") {
                None
            } else {
                Some(
                    Glass::by_name(cols[2])
                        .ok_or_else(|| fail(format!("unknown glass `{}`", cols[2])))?,
                )
            };
            let semi_aperture = if let Some(tok) = cols.get(3) {
                tok.parse::<f64>()
                    .map_err(|_| fail(format!("bad aperture `{tok}`")))?
            } else {
                f64::INFINITY
            };
            surfaces.push(Surface {
                radius,
                thickness,
                glass,
                semi_aperture,
            });
        }
        Self::new(surfaces)
    }

    /// Formats the lens as a prescription that [`Lens::parse`] reads back.
    pub fn to_prescription(&self) -> String {
        let mut lines = vec!["# radius  thickness  glass  semi-aperture".to_owned()];
        for s in &self.surfaces {
            let radius = if s.radius == 0.0 {
                "flat".to_owned()
            } else {
                format!("{:.6}", s.radius)
            };
            let glass = s.glass.map_or("air", |g| g.name);
            let mut line = format!("{radius} {:.6} {glass}", s.thickness);
            if s.semi_aperture.is_finite() {
                line = format!("{line} {}", s.semi_aperture);
            }
            lines.push(line);
        }
        format!("{}\n", lines.join("\n"))
    }

    /// The surfaces, front to back.
    pub fn surfaces(&self) -> &[Surface] {
        &self.surfaces
    }

    /// Copy of this lens with the image plane `distance` behind the last vertex.
    pub fn with_image_distance(&self, distance: f64) -> Self {
        let mut lens = self.clone();
        if let Some(last) = lens.surfaces.last_mut() {
            last.thickness = distance;
        }
        lens
    }

    fn image_z(&self) -> f64 {
        self.surfaces.iter().map(|s| s.thickness).sum()
    }

    /// Paraxial focal length and back focal distance at `nm`, assuming air on both sides.
    /// Returns `None` for an afocal system.
    pub fn paraxial(&self, nm: f64) -> Option<Paraxial> {
        let mut y = 1.0_f64;
        let mut u = 0.0_f64;
        let mut n = 1.0_f64;
        let mut y_last = y;
        for s in &self.surfaces {
            let n_next = s.index(nm);
            u = (n * u - y * s.curvature() * (n_next - n)) / n_next;
            y_last = y;
            y += s.thickness * u;
            n = n_next;
        }
        if u.abs() < 1e-15 {
            return None;
        }
        Some(Paraxial {
            efl: -1.0 / u,
            bfd: -y_last / u,
        })
    }

    /// Traces one ray through every surface and returns it as it leaves the last one.
    pub fn trace(&self, incoming: Ray, nm: f64) -> core::result::Result<Ray, RayFate> {
        let mut ray = Ray::new(incoming.origin, incoming.dir.normalized());
        let mut n = 1.0_f64;
        let mut vertex_z = 0.0_f64;
        for s in &self.surfaces {
            let (point, normal) = s.intersect(&ray, vertex_z).ok_or(RayFate::Missed)?;
            if point.x.hypot(point.y) > s.semi_aperture {
                return Err(RayFate::Vignetted);
            }
            let n_next = s.index(nm);
            let dir = ray
                .dir
                .refract(normal, n / n_next)
                .ok_or(RayFate::TotalInternalReflection)?;
            ray = Ray::new(point, dir);
            n = n_next;
            vertex_z += s.thickness;
        }
        Ok(ray)
    }

    /// Traces a ray and returns where it meets each surface, bracketed by its starting
    /// point and the point where it crosses the image plane. Used to draw ray diagrams.
    pub fn trace_path(&self, incoming: Ray, nm: f64) -> core::result::Result<Vec<Vec3>, RayFate> {
        let mut ray = Ray::new(incoming.origin, incoming.dir.normalized());
        let mut points = vec![ray.origin];
        let mut n = 1.0_f64;
        let mut vertex_z = 0.0_f64;
        for s in &self.surfaces {
            let (point, normal) = s.intersect(&ray, vertex_z).ok_or(RayFate::Missed)?;
            if point.x.hypot(point.y) > s.semi_aperture {
                return Err(RayFate::Vignetted);
            }
            let n_next = s.index(nm);
            let dir = ray
                .dir
                .refract(normal, n / n_next)
                .ok_or(RayFate::TotalInternalReflection)?;
            points.push(point);
            ray = Ray::new(point, dir);
            n = n_next;
            vertex_z += s.thickness;
        }
        if ray.dir.z <= 0.0 {
            return Err(RayFate::Missed);
        }
        let t = (self.image_z() - ray.origin.z) / ray.dir.z;
        points.push(ray.at(t));
        Ok(points)
    }
    /// Distance from the last vertex to where light from an axial object point comes to a
    /// paraxial focus. `object_distance` is measured from the first vertex; zero, negative or
    /// infinite means an object at infinity. A negative result means the image is virtual.
    /// Returns `None` when the light neither converges nor diverges.
    pub fn image_distance(&self, nm: f64, object_distance: f64) -> Option<f64> {
        let mut y = 1.0_f64;
        let mut u = if object_distance.is_finite() && object_distance > 0.0 {
            1.0 / object_distance
        } else {
            0.0
        };
        let mut n = 1.0_f64;
        let mut y_last = y;
        for s in &self.surfaces {
            let n_next = s.index(nm);
            u = (n * u - y * s.curvature() * (n_next - n)) / n_next;
            y_last = y;
            y += s.thickness * u;
            n = n_next;
        }
        if u.abs() < 1e-15 {
            return None;
        }
        Some(-y_last / u)
    }

    /// The same lens turned around, so light travels from the old image side to the old
    /// object side. Assumes air behind the last surface. The new last surface gets
    /// thickness zero.
    pub fn reversed(&self) -> Self {
        let n = self.surfaces.len();
        let surfaces: Vec<Surface> = (0..n)
            .map(|k| {
                let old = n - 1 - k;
                let s = &self.surfaces[old];
                let before = old.checked_sub(1).map(|i| self.surfaces[i]);
                Surface {
                    radius: -s.radius,
                    thickness: before.map_or(0.0, |b| b.thickness),
                    glass: before.and_then(|b| b.glass),
                    semi_aperture: s.semi_aperture,
                }
            })
            .collect();
        Self { surfaces }
    }
    /// Traces a ray and returns where it crosses the image plane.
    pub fn trace_to_image(&self, incoming: Ray, nm: f64) -> core::result::Result<Vec3, RayFate> {
        let out = self.trace(incoming, nm)?;
        if out.dir.z <= 0.0 {
            return Err(RayFate::Missed);
        }
        let t = (self.image_z() - out.origin.z) / out.dir.z;
        Ok(out.at(t))
    }

    /// Traces a square grid of parallel rays (clipped to a circle of radius `pupil_radius`)
    /// arriving at `field_deg` degrees off axis, and records where they land on the image plane.
    pub fn spot_diagram(&self, nm: f64, field_deg: f64, grid: usize, pupil_radius: f64) -> Spot {
        let theta = field_deg.to_radians();
        let dir = Vec3::new(0.0, theta.sin(), theta.cos());
        let mut spot = Spot {
            points: Vec::new(),
            launched: 0,
        };
        for i in 0..grid {
            for j in 0..grid {
                let x = ((i as f64 + 0.5) / grid as f64 * 2.0 - 1.0) * pupil_radius;
                let y = ((j as f64 + 0.5) / grid as f64 * 2.0 - 1.0) * pupil_radius;
                if x * x + y * y > pupil_radius * pupil_radius {
                    continue;
                }
                spot.launched += 1;
                if let Ok(p) = self.trace_to_image(Ray::new(Vec3::new(x, y, 0.0), dir), nm) {
                    spot.points.push((p.x, p.y));
                }
            }
        }
        spot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glass::{BK7, F2};

    fn s(radius: f64, thickness: f64, glass: Option<&'static Glass>) -> Surface {
        Surface {
            radius,
            thickness,
            glass,
            semi_aperture: 1.0e3,
        }
    }

    fn plano_convex() -> Lens {
        Lens::new(vec![s(51.68, 5.0, Some(&BK7)), s(0.0, 90.0, None)]).expect("valid lens")
    }

    #[test]
    fn plano_convex_matches_closed_form() {
        let n = BK7.index(D_LINE_NM);
        let p = plano_convex().paraxial(D_LINE_NM).expect("finite focus");
        let f = 51.68 / (n - 1.0);
        assert!((p.efl - f).abs() < 1e-9);
        assert!((p.bfd - (f - 5.0 / n)).abs() < 1e-9);
    }

    #[test]
    fn biconvex_matches_the_lensmakers_equation() {
        let n = BK7.index(D_LINE_NM);
        let (r1, r2, d) = (100.0, -100.0, 5.0);
        let lens = Lens::new(vec![s(r1, d, Some(&BK7)), s(r2, 50.0, None)]).expect("valid lens");
        let p = lens.paraxial(D_LINE_NM).expect("finite focus");
        let power = (n - 1.0) * (1.0 / r1 - 1.0 / r2 + (n - 1.0) * d / (n * r1 * r2));
        assert!((p.efl - 1.0 / power).abs() < 1e-9);
        let bfd = p.efl * (1.0 - (n - 1.0) * d / (n * r1));
        assert!((p.bfd - bfd).abs() < 1e-9);
    }

    #[test]
    fn blue_focuses_closer_than_red() {
        let lens = plano_convex();
        let blue = lens.paraxial(450.0).expect("finite focus");
        let red = lens.paraxial(650.0).expect("finite focus");
        assert!(blue.bfd < red.bfd);
    }

    #[test]
    fn real_ray_meets_the_paraxial_focus() {
        let lens = plano_convex();
        let p = lens.paraxial(D_LINE_NM).expect("finite focus");
        let focused = lens.with_image_distance(p.bfd);
        let ray = Ray::new(Vec3::new(0.01, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        let hit = focused.trace_to_image(ray, D_LINE_NM).expect("ray passes");
        assert!(hit.x.abs() < 1e-5, "x = {}", hit.x);
        assert!(hit.y.abs() < 1e-9);
    }

    #[test]
    fn spot_is_small_at_focus_and_large_when_defocused() {
        let lens = plano_convex();
        let p = lens.paraxial(D_LINE_NM).expect("finite focus");
        let sharp = lens
            .with_image_distance(p.bfd)
            .spot_diagram(D_LINE_NM, 0.0, 21, 1.0)
            .rms_radius()
            .expect("rays survive");
        let blurred = lens
            .with_image_distance(p.bfd + 5.0)
            .spot_diagram(D_LINE_NM, 0.0, 21, 1.0)
            .rms_radius()
            .expect("rays survive");
        assert!(sharp < 1e-3, "sharp = {sharp}");
        assert!(blurred > 10.0 * sharp, "blurred = {blurred}");
    }

    #[test]
    fn off_axis_field_lands_at_f_tan_theta() {
        let lens = plano_convex();
        let p = lens.paraxial(D_LINE_NM).expect("finite focus");
        let spot = lens
            .with_image_distance(p.bfd)
            .spot_diagram(D_LINE_NM, 1.0, 15, 1.0);
        let (cx, cy) = spot.centroid().expect("rays survive");
        assert!(
            (cy - p.efl * 1.0_f64.to_radians().tan()).abs() < 0.02,
            "cy = {cy}"
        );
        assert!(cx.abs() < 1e-6);
        assert!((spot.transmitted_fraction() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn doublet_reduces_chromatic_focal_shift() {
        let d = D_LINE_NM;
        let (v1, v2) = (BK7.abbe(), F2.abbe());
        let (n1, n2) = (BK7.index(d), F2.index(d));
        let phi = 0.01;
        let phi1 = phi * v1 / (v1 - v2);
        let phi2 = -phi * v2 / (v1 - v2);
        let r1 = 2.0 * (n1 - 1.0) / phi1;
        let r3 = 1.0 / (-1.0 / r1 - phi2 / (n2 - 1.0));
        let doublet = Lens::new(vec![
            s(r1, 0.5, Some(&BK7)),
            s(-r1, 0.3, Some(&F2)),
            s(r3, 50.0, None),
        ])
        .expect("valid lens");
        let r_single = 2.0 * (n1 - 1.0) / phi;
        let singlet = Lens::new(vec![s(r_single, 0.5, Some(&BK7)), s(-r_single, 50.0, None)])
            .expect("valid lens");
        let shift = |lens: &Lens| {
            let blue = lens.paraxial(450.0).expect("finite focus").bfd;
            let red = lens.paraxial(650.0).expect("finite focus").bfd;
            (blue - red).abs()
        };
        assert!(shift(&doublet) < 0.5 * shift(&singlet));
    }

    #[test]
    fn rays_can_miss_or_be_vignetted() {
        let tiny = Lens::new(vec![s(2.0, 5.0, Some(&BK7)), s(0.0, 5.0, None)]).expect("valid lens");
        let far = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(tiny.trace(far, 550.0).err(), Some(RayFate::Missed));

        let stopped = Lens::new(vec![
            Surface {
                radius: 50.0,
                thickness: 5.0,
                glass: Some(&BK7),
                semi_aperture: 1.0,
            },
            Surface {
                radius: 0.0,
                thickness: 90.0,
                glass: None,
                semi_aperture: 1.0,
            },
        ])
        .expect("valid lens");
        let wide = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(stopped.trace(wide, 550.0).err(), Some(RayFate::Vignetted));
        let narrow = Ray::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        assert!(stopped.trace(narrow, 550.0).is_ok());
    }

    #[test]
    fn path_has_one_point_per_surface_plus_both_ends() {
        let lens = plano_convex();
        let focus = lens.paraxial(D_LINE_NM).expect("finite focus");
        let focused = lens.with_image_distance(focus.bfd);
        let ray = Ray::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        let path = focused.trace_path(ray, D_LINE_NM).expect("ray passes");
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], ray.origin);
        assert!((path[3].z - (5.0 + focus.bfd)).abs() < 1e-9);
        assert!(path[3].y.abs() < 0.01, "y = {}", path[3].y);
        let image = focused.trace_to_image(ray, D_LINE_NM).expect("ray passes");
        assert!(path[3].near(image, 1e-12));
    }

    #[test]
    fn path_reports_why_a_ray_failed() {
        let tiny = Lens::new(vec![s(2.0, 5.0, Some(&BK7)), s(0.0, 5.0, None)]).expect("valid lens");
        let far = Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(tiny.trace_path(far, 550.0).err(), Some(RayFate::Missed));
    }
    #[test]
    fn image_distance_at_infinity_is_the_back_focal_distance() {
        let lens = plano_convex();
        let bfd = lens.paraxial(D_LINE_NM).expect("finite focus").bfd;
        let at_infinity = lens
            .image_distance(D_LINE_NM, f64::INFINITY)
            .expect("finite focus");
        let at_zero = lens.image_distance(D_LINE_NM, 0.0).expect("finite focus");
        assert!((at_infinity - bfd).abs() < 1e-9);
        assert!((at_zero - bfd).abs() < 1e-9);
    }

    #[test]
    fn nearer_objects_focus_farther_behind_the_lens() {
        let lens = plano_convex();
        let infinity = lens
            .image_distance(D_LINE_NM, f64::INFINITY)
            .expect("finite focus");
        let far = lens.image_distance(D_LINE_NM, 10_000.0).expect("focus");
        let near = lens.image_distance(D_LINE_NM, 500.0).expect("focus");
        assert!(near > far, "near {near}, far {far}");
        assert!(far > infinity, "far {far}, infinity {infinity}");
    }

    #[test]
    fn image_distance_follows_the_thin_lens_equation() {
        let thin = Lens::new(vec![s(100.0, 0.001, Some(&BK7)), s(-100.0, 90.0, None)])
            .expect("valid lens");
        let f = thin.paraxial(D_LINE_NM).expect("finite focus").efl;
        let v = thin.image_distance(D_LINE_NM, 1000.0).expect("focus");
        assert!((v - 1.0 / (1.0 / f - 1.0 / 1000.0)).abs() < 0.01, "v = {v}");
    }

    #[test]
    fn reversing_twice_restores_the_lens() {
        let lens = Lens::parse("44.78 4.0 N-BK7 12.5\n-44.78 2.5 F2 12.5\n-812 95.0 air 12.5\n")
            .expect("valid lens");
        let again = lens.reversed().reversed();
        assert_eq!(again.surfaces().len(), 3);
        for (a, b) in lens.surfaces().iter().zip(again.surfaces()) {
            assert!((a.radius - b.radius).abs() < 1e-12);
            assert_eq!(a.glass.map(|g| g.name), b.glass.map(|g| g.name));
            assert!((a.semi_aperture - b.semi_aperture).abs() < 1e-12);
        }
        assert!((again.surfaces()[0].thickness - 4.0).abs() < 1e-12);
        assert!((again.surfaces()[1].thickness - 2.5).abs() < 1e-12);
    }

    #[test]
    fn reversed_lens_has_the_same_focal_length() {
        let lens = Lens::parse("44.78 4.0 N-BK7 12.5\n-44.78 2.5 F2 12.5\n-812 95.0 air 12.5\n")
            .expect("valid lens");
        let a = lens.paraxial(D_LINE_NM).expect("finite focus");
        let b = lens.reversed().paraxial(D_LINE_NM).expect("finite focus");
        assert!((a.efl - b.efl).abs() < 1e-9, "{} and {}", a.efl, b.efl);
    }

    #[test]
    fn rays_retrace_their_path_through_the_reversed_lens() {
        let lens = Lens::parse("44.78 4.0 N-BK7 12.5\n-44.78 2.5 F2 12.5\n-812 95.0 air 12.5\n")
            .expect("valid lens");
        let incoming = Ray::new(Vec3::new(0.0, 6.0, -10.0), Vec3::new(0.0, 0.02, 1.0));
        let path = lens.trace_path(incoming, 550.0).expect("ray passes");
        let out = lens.trace(incoming, 550.0).expect("ray passes");
        let z0: f64 = lens.surfaces()[..2].iter().map(|s| s.thickness).sum();
        let back = Ray::new(
            Vec3::new(out.origin.x, out.origin.y, z0 - out.origin.z),
            Vec3::new(-out.dir.x, -out.dir.y, out.dir.z),
        );
        let returned = lens
            .reversed()
            .trace(back, 550.0)
            .expect("the reversed ray passes");
        let first = path[1];
        assert!(
            returned
                .origin
                .near(Vec3::new(first.x, first.y, z0 - first.z), 1e-6),
            "{:?} versus {first:?}",
            returned.origin
        );
        let d = incoming.dir.normalized();
        assert!(
            returned.dir.near(Vec3::new(-d.x, -d.y, d.z), 1e-9),
            "{:?} versus {d:?}",
            returned.dir
        );
    }
    #[test]
    fn image_distance_can_be_changed() {
        let lens = plano_convex().with_image_distance(7.5);
        let last = lens.surfaces()[1];
        assert!((last.thickness - 7.5).abs() < 1e-12);
    }

    #[test]
    fn parses_a_prescription() {
        let text = "# demo\n51.68 5.0 N-BK7 12.5   # trailing comment\n\nflat 90 air\n";
        let lens = Lens::parse(text).expect("valid prescription");
        assert_eq!(lens.surfaces().len(), 2);
        let first = lens.surfaces()[0];
        assert_eq!(first.glass.map(|g| g.name), Some("N-BK7"));
        assert!((first.semi_aperture - 12.5).abs() < 1e-12);
        let second = lens.surfaces()[1];
        assert!(second.glass.is_none());
        assert!(second.semi_aperture.is_infinite());
        assert!(second.curvature().abs() < 1e-30);
    }

    #[test]
    fn rejects_bad_prescriptions() {
        assert!(matches!(
            Lens::parse("1 2 unobtainium"),
            Err(PrismError::Parse { line: 1, .. })
        ));
        assert!(matches!(
            Lens::parse("# c\n1 abc air"),
            Err(PrismError::Parse { line: 2, .. })
        ));
        assert!(matches!(
            Lens::parse("1 2"),
            Err(PrismError::Parse { line: 1, .. })
        ));
        assert!(matches!(
            Lens::parse("# nothing\n"),
            Err(PrismError::InvalidParameter { .. })
        ));
        assert!(matches!(
            Lens::parse("1 -2 air"),
            Err(PrismError::InvalidParameter { .. })
        ));
    }
}
