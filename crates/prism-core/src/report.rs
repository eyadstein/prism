//! Text report of a lens design, shared by the command line tool and the WebAssembly API.

use crate::analysis::{field_point, mtf_sagittal, mtf_tangential};
use crate::lens::{Lens, D_LINE_NM};

/// Focal length, spot sizes, distortion, field curvature and MTF of `lens`, as text.
/// Returns `None` for a lens with no finite focal length.
pub fn lens_report(lens: &Lens, name: &str, pupil: f64, field: f64) -> Option<String> {
    let p = lens.paraxial(D_LINE_NM)?;
    let mut lines = vec![
        format!("{name}: {} surfaces", lens.surfaces().len()),
        format!(
            "effective focal length {:.3}, back focal distance {:.3} (paraxial, d line)",
            p.efl, p.bfd
        ),
        format!("spot diagrams at the paraxial focus: pupil radius {pupil}, field {field} deg"),
    ];
    let focused = lens.with_image_distance(p.bfd);
    for nm in [450.0, 550.0, 650.0] {
        let text = focused
            .spot_diagram(nm, field, 41, pupil)
            .rms_radius()
            .map_or_else(
                || "every ray was blocked".to_owned(),
                |r| format!("RMS spot radius {:.2} um", r * 1000.0),
            );
        lines.push(format!("  {nm:.0} nm  {text}"));
    }
    lines.push("field analysis at 550 nm, image plane at the paraxial focus:".to_owned());
    lines.push("  field  ideal mm   real mm  distortion %  tangential mm  sagittal mm".to_owned());
    for angle in [0.0, 5.0, 10.0] {
        if let Some(pt) = field_point(&focused, 550.0, angle, pupil, 41) {
            lines.push(format!(
                "  {:>5.1}  {:>8.4}  {:>8.4}  {:>12.4}  {:>13.4}  {:>11.4}",
                pt.field_deg,
                pt.ideal_height,
                pt.real_height,
                pt.distortion_percent,
                pt.tangential_focus,
                pt.sagittal_focus
            ));
        } else {
            lines.push(format!("  {angle:>5.1}  rays blocked"));
        }
    }
    let spot = focused.spot_diagram(550.0, 0.0, 41, pupil);
    lines.push("geometric MTF on axis at 550 nm (tangential / sagittal):".to_owned());
    for lp in [5.0, 10.0, 20.0, 50.0] {
        lines.push(format!(
            "  {lp:>4.0} lp/mm  {:.3} / {:.3}",
            mtf_tangential(&spot, lp),
            mtf_sagittal(&spot, lp)
        ));
    }
    Some(format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_for_the_plano_convex_example() {
        let lens = Lens::parse(include_str!("../../../examples/plano-convex.lens")).expect("valid");
        let text = lens_report(&lens, "demo", 5.0, 0.0).expect("finite focus");
        assert!(text.starts_with("demo: 2 surfaces\n"));
        assert!(text.contains("effective focal length 100.000"));
        assert!(text.contains("450 nm"));
        assert!(text.contains("650 nm"));
        assert!(text.contains("lp/mm"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn afocal_lenses_have_no_report() {
        let plate = Lens::parse("flat 5 N-BK7\nflat 10 air\n").expect("valid lens");
        assert!(lens_report(&plate, "plate", 5.0, 0.0).is_none());
    }

    #[test]
    fn blocked_pupils_are_reported_not_hidden() {
        let lens = Lens::parse("51.68 5.0 N-BK7 0.1\nflat 90 air 0.1\n").expect("valid lens");
        let text = lens_report(&lens, "pinhole", 5.0, 0.0).expect("finite focus");
        assert!(text.contains("rays blocked") || text.contains("every ray was blocked"));
    }
}
