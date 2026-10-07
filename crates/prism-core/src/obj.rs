//! Wavefront OBJ parsing. Only geometry is read: `v` and `f` lines. Everything else
//! (normals, texture coordinates, groups, materials) is ignored.

use std::cmp::Ordering;

use crate::error::{PrismError, Result};
use crate::geometry::Triangle;
use crate::math::Vec3;

fn fail(line: usize, reason: String) -> PrismError {
    PrismError::Parse { line, reason }
}

fn coordinate(word: Option<&str>, line: usize) -> Result<f64> {
    let Some(w) = word else {
        return Err(fail(line, "vertex needs three coordinates".to_owned()));
    };
    let value = w
        .parse::<f64>()
        .map_err(|_| fail(line, format!("bad coordinate `{w}`")))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(fail(line, format!("coordinate `{w}` must be finite")))
    }
}

/// Turns a face corner such as `3`, `3/1`, `3//2` or `-1` into a zero-based vertex index.
fn resolve(token: &str, count: usize, line: usize) -> Result<usize> {
    let head = token.split('/').next().unwrap_or("");
    let value = head
        .parse::<i64>()
        .map_err(|_| fail(line, format!("bad vertex index `{token}`")))?;
    let total = i64::try_from(count).map_err(|_| fail(line, "too many vertices".to_owned()))?;
    let zero_based = match value.cmp(&0) {
        Ordering::Greater => value - 1,
        Ordering::Less => total + value,
        Ordering::Equal => return Err(fail(line, "vertex indices start at 1".to_owned())),
    };
    if !(0..total).contains(&zero_based) {
        return Err(fail(
            line,
            format!("vertex index `{token}` is out of range"),
        ));
    }
    Ok(zero_based as usize)
}

/// Parses OBJ text into triangles. Faces with more than three corners are fan-triangulated.
/// Negative indices count back from the most recent vertex, as the format specifies.
pub fn parse_obj(text: &str) -> Result<Vec<Triangle>> {
    let mut vertices: Vec<Vec3> = Vec::new();
    let mut triangles = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let mut words = raw.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("v") => {
                let x = coordinate(words.next(), number)?;
                let y = coordinate(words.next(), number)?;
                let z = coordinate(words.next(), number)?;
                vertices.push(Vec3::new(x, y, z));
            }
            Some("f") => {
                let mut corners = Vec::new();
                for token in words {
                    corners.push(resolve(token, vertices.len(), number)?);
                }
                if corners.len() < 3 {
                    return Err(fail(number, "face needs at least three corners".to_owned()));
                }
                for k in 1..corners.len() - 1 {
                    triangles.push(Triangle::new(
                        vertices[corners[0]],
                        vertices[corners[k]],
                        vertices[corners[k + 1]],
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
f 1 2 3 4\nf 5 8 7 6\nf 1 5 6 2\nf 2 6 7 3\nf 3 7 8 4\nf 4 8 5 1\n";

    fn error_line(text: &str) -> Option<usize> {
        if let Err(PrismError::Parse { line, .. }) = parse_obj(text) {
            Some(line)
        } else {
            None
        }
    }

    #[test]
    fn quads_become_two_triangles_each() {
        let tris = parse_obj(CUBE).expect("valid cube");
        assert_eq!(tris.len(), 12);
        assert_eq!(tris[0].a, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(tris[0].b, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(tris[0].c, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(tris[1].c, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn negative_and_slashed_indices() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\nf 1/1/1 2//2 3\n";
        let tris = parse_obj(text).expect("valid");
        assert_eq!(tris.len(), 2);
        assert_eq!(tris[0], tris[1]);
        assert_eq!(tris[0].c, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn other_directives_and_comments_are_ignored() {
        let text = "# header\nmtllib x.mtl\no thing\nv 0 0 0 1.0\nv 1 0 0\nv 0 1 0\n\
vn 0 0 1\nvt 0 0\nusemtl m\ns off\nf 1 2 3 # trailing\n";
        assert_eq!(parse_obj(text).expect("valid").len(), 1);
        assert_eq!(parse_obj("").expect("empty is fine").len(), 0);
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        assert_eq!(error_line("v 0 0 0\nf 1 2 3\n"), Some(2));
        assert_eq!(error_line("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n"), Some(4));
        assert_eq!(error_line("v 0 0 0\nv 1 0 0\nf 1 2\n"), Some(3));
        assert_eq!(error_line("v 0 0 x\n"), Some(1));
        assert_eq!(error_line("# c\nv 0 0\n"), Some(2));
        assert_eq!(error_line("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 q\n"), Some(4));
        assert_eq!(error_line("v 0 0 inf\n"), Some(1));
    }

    #[test]
    fn octahedron_example_faces_point_outward() {
        let tris = parse_obj(include_str!("../../../examples/octahedron.obj")).expect("valid");
        assert_eq!(tris.len(), 8);
        for t in &tris {
            let normal = (t.b - t.a).cross(t.c - t.a);
            let centroid = (t.a + t.b + t.c) / 3.0;
            assert!(normal.dot(centroid) > 0.0, "face points inward: {t:?}");
        }
    }
}
