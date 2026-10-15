//! SVG cross-section drawings of a lens with rays traced at three wavelengths.

use crate::lens::{Lens, Surface, D_LINE_NM};
use crate::math::{Ray, Vec3};

const WIDTH: f64 = 960.0;
const HEIGHT: f64 = 320.0;
const MARGIN: f64 = 24.0;
const CURVE_STEPS: usize = 32;
const RAY_LEAD: f64 = 10.0;
const MAX_RAYS: usize = 101;
const WAVELENGTHS: [(f64, &str); 3] = [(450.0, "#5aa9ff"), (550.0, "#6fcf6f"), (650.0, "#ff6a55")];

struct View {
    z_min: f64,
    scale_z: f64,
    scale_y: f64,
}

impl View {
    fn point(&self, z: f64, y: f64) -> (f64, f64) {
        (
            MARGIN + (z - self.z_min) * self.scale_z,
            HEIGHT / 2.0 - y * self.scale_y,
        )
    }
}

/// Axial position of a spherical surface at height `y`; flat surfaces return the vertex.
fn surface_z(surface: &Surface, vertex_z: f64, y: f64) -> f64 {
    let r = surface.radius;
    if r == 0.0 || !r.is_finite() {
        return vertex_z;
    }
    let h = y.abs().min(r.abs());
    vertex_z + r - r.signum() * (r * r - h * h).sqrt()
}

fn profile(surface: &Surface, vertex_z: f64, aperture: f64, view: &View) -> Vec<(f64, f64)> {
    (0..=CURVE_STEPS)
        .map(|k| {
            let y = -aperture + 2.0 * aperture * k as f64 / CURVE_STEPS as f64;
            view.point(surface_z(surface, vertex_z, y), y)
        })
        .collect()
}

fn points(pts: &[(f64, f64)]) -> String {
    pts.iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Draws `lens` with its image plane at the d-line paraxial focus, tracing `rays` rays across a
/// pupil of radius `pupil` at 450, 550 and 650 nm. Returns `None` for a lens without a focus
/// behind it, or for unusable arguments (`rays` must be 1 to 101, `pupil` positive).
pub fn lens_svg(lens: &Lens, rays: usize, pupil: f64, field_deg: f64) -> Option<String> {
    if rays == 0 || rays > MAX_RAYS || !(pupil.is_finite() && pupil > 0.0) || !field_deg.is_finite()
    {
        return None;
    }
    let focus = lens.paraxial(D_LINE_NM)?;
    if !(focus.bfd.is_finite() && focus.bfd > 0.0) {
        return None;
    }
    let lens = lens.with_image_distance(focus.bfd);
    let surfaces = lens.surfaces();
    let image_z: f64 = surfaces.iter().map(|s| s.thickness).sum();
    let mut vertices = Vec::with_capacity(surfaces.len());
    let mut z = 0.0;
    for s in surfaces {
        vertices.push(z);
        z += s.thickness;
    }
    let apertures: Vec<f64> = surfaces
        .iter()
        .map(|s| s.semi_aperture.min(pupil * 1.5))
        .collect();

    let theta = field_deg.to_radians();
    let dir = Vec3::new(0.0, theta.sin(), theta.cos());
    let mut traced: Vec<(&str, Vec<Vec<Vec3>>)> = Vec::new();
    for (nm, color) in WAVELENGTHS {
        let mut paths = Vec::new();
        for k in 0..rays {
            let u = if rays == 1 {
                0.0
            } else {
                (k as f64 / (rays - 1) as f64 * 2.0 - 1.0) * pupil
            };
            let start = Vec3::new(0.0, u, 0.0) - dir * RAY_LEAD;
            if let Ok(path) = lens.trace_path(Ray::new(start, dir), nm) {
                paths.push(path);
            }
        }
        traced.push((color, paths));
    }

    let mut z_min = 0.0_f64;
    let mut z_max = image_z;
    let mut y_max = apertures.iter().copied().fold(0.0_f64, f64::max);
    for (i, s) in surfaces.iter().enumerate() {
        let edge = surface_z(s, vertices[i], apertures[i]);
        z_min = z_min.min(edge);
        z_max = z_max.max(edge);
    }
    for (_, paths) in &traced {
        for path in paths {
            for p in path {
                z_min = z_min.min(p.z);
                z_max = z_max.max(p.z);
                y_max = y_max.max(p.y.abs());
            }
        }
    }
    let scale_z = (WIDTH - 2.0 * MARGIN) / (z_max - z_min).max(1e-9);
    let scale_y = ((HEIGHT - 2.0 * MARGIN) / (2.0 * y_max.max(1e-9))).min(4.0 * scale_z);
    let view = View {
        z_min,
        scale_z,
        scale_y,
    };

    let mut lines = vec![
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\" role=\"img\" aria-label=\"Lens cross-section with rays traced at 450, 550 and 650 nanometres\">"
        ),
        "<rect width=\"100%\" height=\"100%\" fill=\"#0d0f0e\"/>".to_owned(),
    ];
    let (axis_start, _) = view.point(z_min, 0.0);
    let (axis_end, axis_y) = view.point(z_max, 0.0);
    lines.push(format!(
        "<line x1=\"{axis_start:.2}\" y1=\"{axis_y:.2}\" x2=\"{axis_end:.2}\" y2=\"{axis_y:.2}\" stroke=\"#2a2d2b\" stroke-dasharray=\"4 4\"/>"
    ));
    for i in 0..surfaces.len().saturating_sub(1) {
        if surfaces[i].glass.is_none() {
            continue;
        }
        let aperture = apertures[i].min(apertures[i + 1]);
        let mut outline = profile(&surfaces[i], vertices[i], aperture, &view);
        let mut back = profile(&surfaces[i + 1], vertices[i + 1], aperture, &view);
        back.reverse();
        outline.extend(back);
        lines.push(format!(
            "<polygon points=\"{}\" fill=\"#d6d3c8\" fill-opacity=\"0.12\"/>",
            points(&outline)
        ));
    }
    for (i, surface) in surfaces.iter().enumerate() {
        let curve = profile(surface, vertices[i], apertures[i], &view);
        lines.push(format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#8d8f8a\" stroke-width=\"1.5\"/>",
            points(&curve)
        ));
    }
    let (plane_x, _) = view.point(image_z, 0.0);
    lines.push(format!(
        "<line x1=\"{plane_x:.2}\" y1=\"{MARGIN:.2}\" x2=\"{plane_x:.2}\" y2=\"{:.2}\" stroke=\"#6b6e69\"/>",
        HEIGHT - MARGIN
    ));
    for (color, paths) in &traced {
        for path in paths {
            let pts: Vec<(f64, f64)> = path.iter().map(|p| view.point(p.z, p.y)).collect();
            lines.push(format!(
                "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.2\" stroke-opacity=\"0.75\"/>",
                points(&pts)
            ));
        }
    }
    lines.push("</svg>".to_owned());
    Some(format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plano() -> Lens {
        Lens::parse(include_str!("../../../examples/plano-convex.lens")).expect("valid lens")
    }

    fn doublet() -> Lens {
        Lens::parse(include_str!("../../../examples/doublet.lens")).expect("valid lens")
    }

    fn all_points(svg: &str) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for chunk in svg.split("points=\"").skip(1) {
            let list = chunk.split('"').next().unwrap_or("");
            for pair in list.split_whitespace() {
                let mut it = pair.split(',');
                let x: f64 = it.next().expect("x").parse().expect("a number");
                let y: f64 = it.next().expect("y").parse().expect("a number");
                out.push((x, y));
            }
        }
        out
    }

    #[test]
    fn draws_the_expected_elements() {
        let svg = lens_svg(&plano(), 11, 5.0, 0.0).expect("draws");
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>\n"));
        assert_eq!(svg.matches("<polygon").count(), 1);
        assert_eq!(svg.matches("<polyline").count(), 2 + 3 * 11);
        let svg = lens_svg(&doublet(), 11, 5.0, 0.0).expect("draws");
        assert_eq!(svg.matches("<polygon").count(), 2);
        assert_eq!(svg.matches("<polyline").count(), 3 + 3 * 11);
    }

    #[test]
    fn every_coordinate_is_inside_the_canvas() {
        for field in [0.0, 8.0] {
            let svg = lens_svg(&doublet(), 7, 6.0, field).expect("draws");
            let pts = all_points(&svg);
            assert!(pts.len() > 100);
            for (x, y) in pts {
                assert!((0.0..=WIDTH).contains(&x), "x = {x}");
                assert!((0.0..=HEIGHT).contains(&y), "y = {y}");
            }
            assert!(!svg.contains("NaN"));
            assert!(!svg.contains("inf"));
        }
    }

    #[test]
    fn one_ray_is_the_axial_ray() {
        let svg = lens_svg(&plano(), 1, 5.0, 0.0).expect("draws");
        assert_eq!(svg.matches("<polyline").count(), 2 + 3);
    }

    #[test]
    fn blocked_rays_are_left_out() {
        let pinhole = Lens::parse("51.68 5.0 N-BK7 0.1\nflat 90 air 0.1\n").expect("valid lens");
        let svg = lens_svg(&pinhole, 10, 5.0, 0.0).expect("draws");
        assert_eq!(svg.matches("<polyline").count(), 2);
    }

    #[test]
    fn rejects_unusable_requests() {
        assert!(lens_svg(&plano(), 0, 5.0, 0.0).is_none());
        assert!(lens_svg(&plano(), 102, 5.0, 0.0).is_none());
        assert!(lens_svg(&plano(), 11, 0.0, 0.0).is_none());
        assert!(lens_svg(&plano(), 11, 5.0, f64::NAN).is_none());
        let plate = Lens::parse("flat 5 N-BK7 12\nflat 10 air 12\n").expect("valid lens");
        assert!(lens_svg(&plate, 11, 5.0, 0.0).is_none());
    }

    #[test]
    fn surface_profile_follows_the_sphere() {
        let convex = Surface {
            radius: 50.0,
            thickness: 1.0,
            glass: None,
            semi_aperture: 10.0,
        };
        assert!((surface_z(&convex, 2.0, 0.0) - 2.0).abs() < 1e-12);
        assert!((surface_z(&convex, 2.0, 5.0) - (2.0 + 0.250_628_9)).abs() < 1e-5);
        let flat = Surface {
            radius: 0.0,
            ..convex
        };
        assert!((surface_z(&flat, 2.0, 5.0) - 2.0).abs() < 1e-12);
    }
}
