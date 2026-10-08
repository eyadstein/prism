//! Text scene description files, one directive per line; `#` starts a comment.
//!
//! - `sky <horizon> <zenith>`: background radiance, default 1 and 1.
//! - `camera <ex ey ez> <tx ty tz> <fov degrees>`: required; y is up.
//! - `image <width> <height> <samples>`: default render settings.
//! - `floor <half extent> <light> <dark>`: checkerboard of 1 by 1 tiles on y = 0.
//! - `sphere <x y z> <radius> <material>`.
//! - `triangle <x y z> <x y z> <x y z> <material>`.
//! - `mesh <file.obj> <scale> <x y z> <material>`: glass meshes must be closed with
//!   counter-clockwise faces seen from outside.
//!
//! A material is `diffuse <r>`, `mirror <r>` (reflectance 0 to 1), `glass <name>`, or
//! `thinfilm <index> <thickness nm>` (an infinitely thin film such as a soap bubble wall).

use std::collections::HashMap;
use std::str::SplitWhitespace;

use crate::camera::Camera;
use crate::error::{PrismError, Result};
use crate::geometry::{Primitive, Sphere, Triangle};
use crate::glass::Glass;
use crate::material::Material;
use crate::math::Vec3;
use crate::obj::parse_obj;
use crate::scene::{Scene, Sky};

const MAX_FLOOR_HALF_EXTENT: usize = 64;
const MAX_IMAGE_SIDE: usize = 8192;
const MAX_SAMPLES: u32 = 65_536;

/// Reads the text of a mesh file by name. Errors are plain messages.
pub type MeshLoader<'a> = &'a mut dyn FnMut(&str) -> std::result::Result<String, String>;

/// Where the camera stands and what it looks at. The up direction is always +y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSpec {
    /// Eye position.
    pub eye: Vec3,
    /// Point the camera looks at.
    pub target: Vec3,
    /// Vertical field of view in degrees.
    pub fov_degrees: f64,
}

impl CameraSpec {
    /// Builds the camera for an image with the given aspect ratio (width over height).
    pub fn camera(&self, aspect: f64) -> Camera {
        Camera::new(
            self.eye,
            self.target,
            Vec3::new(0.0, 1.0, 0.0),
            self.fov_degrees,
            aspect,
        )
    }
}

/// Default render settings stored in a scene file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageSpec {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Spectral samples per pixel.
    pub samples: u32,
}

/// A parsed scene file.
#[derive(Clone, Debug)]
pub struct SceneFile {
    /// The geometry, materials and sky.
    pub scene: Scene,
    /// The camera.
    pub camera: CameraSpec,
    /// Render defaults, when the file has an `image` directive.
    pub image: Option<ImageSpec>,
}

struct Line<'a> {
    number: usize,
    words: SplitWhitespace<'a>,
}

impl<'a> Line<'a> {
    fn error(&self, reason: impl Into<String>) -> PrismError {
        PrismError::Parse {
            line: self.number,
            reason: reason.into(),
        }
    }

    fn word(&mut self, what: &str) -> Result<&'a str> {
        self.words
            .next()
            .ok_or_else(|| self.error(format!("missing {what}")))
    }

    fn number(&mut self, what: &str) -> Result<f64> {
        let w = self.word(what)?;
        let v = w
            .parse::<f64>()
            .map_err(|_| self.error(format!("bad {what} `{w}`")))?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(self.error(format!("{what} must be finite")))
        }
    }

    fn whole(&mut self, what: &str) -> Result<usize> {
        let w = self.word(what)?;
        w.parse::<usize>()
            .map_err(|_| self.error(format!("{what} must be a whole number, found `{w}`")))
    }

    fn vec3(&mut self, what: &str) -> Result<Vec3> {
        let x = self.number(what)?;
        let y = self.number(what)?;
        let z = self.number(what)?;
        Ok(Vec3::new(x, y, z))
    }

    fn positive(&mut self, what: &str) -> Result<f64> {
        let v = self.number(what)?;
        if v > 0.0 {
            Ok(v)
        } else {
            Err(self.error(format!("{what} must be positive")))
        }
    }

    fn non_negative(&mut self, what: &str) -> Result<f64> {
        let v = self.number(what)?;
        if v >= 0.0 {
            Ok(v)
        } else {
            Err(self.error(format!("{what} must not be negative")))
        }
    }

    fn unit(&mut self, what: &str) -> Result<f64> {
        let v = self.number(what)?;
        if (0.0..=1.0).contains(&v) {
            Ok(v)
        } else {
            Err(self.error(format!("{what} must be between 0 and 1")))
        }
    }

    fn finish(&mut self) -> Result<()> {
        match self.words.next() {
            Some(extra) => Err(self.error(format!("unexpected `{extra}`"))),
            None => Ok(()),
        }
    }
}

#[derive(Default)]
struct Groups {
    index: HashMap<String, usize>,
    groups: Vec<(Material, Vec<Primitive>)>,
}

impl Groups {
    /// Adds primitives to the group with this key, creating it when new. Equal materials
    /// share one BVH, which keeps traversal fast.
    fn add(&mut self, key: String, material: Material, prims: Vec<Primitive>) {
        if let Some(&slot) = self.index.get(&key) {
            self.groups[slot].1.extend(prims);
        } else {
            self.index.insert(key, self.groups.len());
            self.groups.push((material, prims));
        }
    }
}

fn parse_material(line: &mut Line<'_>) -> Result<(String, Material)> {
    let kind = line.word("material")?;
    match kind {
        "diffuse" | "mirror" => {
            let r = line.unit("reflectance")?;
            let material = if kind == "diffuse" {
                Material::Diffuse { reflectance: r }
            } else {
                Material::Mirror { reflectance: r }
            };
            Ok((format!("{kind}:{r}"), material))
        }
        "thinfilm" => {
            let index = line.number("film index")?;
            if index < 1.0 {
                return Err(line.error("film index must be at least 1"));
            }
            let thickness_nm = line.non_negative("film thickness in nm")?;
            Ok((
                format!("thinfilm:{index}:{thickness_nm}"),
                Material::ThinFilm {
                    index,
                    thickness_nm,
                },
            ))
        }
        "glass" => {
            let name = line.word("glass name")?;
            let glass = Glass::by_name(name)
                .ok_or_else(|| line.error(format!("unknown glass `{name}`")))?;
            Ok((format!("glass:{}", glass.name), Material::Dielectric(glass)))
        }
        other => Err(line.error(format!(
            "unknown material `{other}` (use diffuse, mirror, glass or thinfilm)"
        ))),
    }
}

fn parse_sky(line: &mut Line<'_>) -> Result<Sky> {
    let horizon = line.non_negative("horizon radiance")?;
    let zenith = line.non_negative("zenith radiance")?;
    line.finish()?;
    Ok(Sky { horizon, zenith })
}

fn parse_camera(line: &mut Line<'_>) -> Result<CameraSpec> {
    let eye = line.vec3("eye position")?;
    let target = line.vec3("target position")?;
    let fov_degrees = line.number("field of view")?;
    line.finish()?;
    if !(1.0..180.0).contains(&fov_degrees) {
        return Err(line.error("field of view must be at least 1 and below 180 degrees"));
    }
    let forward = (target - eye).normalized();
    if forward.cross(Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6 {
        return Err(line.error("camera must look at a different point and not straight up or down"));
    }
    Ok(CameraSpec {
        eye,
        target,
        fov_degrees,
    })
}

fn parse_image(line: &mut Line<'_>) -> Result<ImageSpec> {
    let width = line.whole("image width")?;
    let height = line.whole("image height")?;
    let samples = line.whole("samples per pixel")?;
    line.finish()?;
    let samples = u32::try_from(samples).unwrap_or(0);
    if !(1..=MAX_IMAGE_SIDE).contains(&width)
        || !(1..=MAX_IMAGE_SIDE).contains(&height)
        || !(1..=MAX_SAMPLES).contains(&samples)
    {
        return Err(line.error(format!(
            "image sides must be 1 to {MAX_IMAGE_SIDE} and samples 1 to {MAX_SAMPLES}"
        )));
    }
    Ok(ImageSpec {
        width,
        height,
        samples,
    })
}

fn checker_floor(half: i32) -> (Vec<Primitive>, Vec<Primitive>) {
    let mut light = Vec::new();
    let mut dark = Vec::new();
    for i in -half..half {
        for j in -half..half {
            let (x, z) = (f64::from(i), f64::from(j));
            let a = Vec3::new(x, 0.0, z);
            let b = Vec3::new(x + 1.0, 0.0, z);
            let c = Vec3::new(x + 1.0, 0.0, z + 1.0);
            let d = Vec3::new(x, 0.0, z + 1.0);
            let tiles = if (i + j).rem_euclid(2) == 0 {
                &mut light
            } else {
                &mut dark
            };
            tiles.push(Primitive::Triangle(Triangle::new(a, b, c)));
            tiles.push(Primitive::Triangle(Triangle::new(a, c, d)));
        }
    }
    (light, dark)
}

fn parse_floor(line: &mut Line<'_>, groups: &mut Groups) -> Result<()> {
    let half = line.whole("floor half extent")?;
    let light = line.unit("light tile reflectance")?;
    let dark = line.unit("dark tile reflectance")?;
    line.finish()?;
    if !(1..=MAX_FLOOR_HALF_EXTENT).contains(&half) {
        return Err(line.error(format!(
            "floor half extent must be 1 to {MAX_FLOOR_HALF_EXTENT}"
        )));
    }
    let half = i32::try_from(half).map_err(|_| line.error("floor half extent is too large"))?;
    let (light_tiles, dark_tiles) = checker_floor(half);
    groups.add(
        format!("diffuse:{light}"),
        Material::Diffuse { reflectance: light },
        light_tiles,
    );
    groups.add(
        format!("diffuse:{dark}"),
        Material::Diffuse { reflectance: dark },
        dark_tiles,
    );
    Ok(())
}

fn parse_sphere(line: &mut Line<'_>, groups: &mut Groups) -> Result<()> {
    let center = line.vec3("sphere centre")?;
    let radius = line.positive("sphere radius")?;
    let (key, material) = parse_material(line)?;
    line.finish()?;
    groups.add(
        key,
        material,
        vec![Primitive::Sphere(Sphere::new(center, radius))],
    );
    Ok(())
}

fn parse_triangle(line: &mut Line<'_>, groups: &mut Groups) -> Result<()> {
    let a = line.vec3("triangle vertex")?;
    let b = line.vec3("triangle vertex")?;
    let c = line.vec3("triangle vertex")?;
    let (key, material) = parse_material(line)?;
    line.finish()?;
    groups.add(
        key,
        material,
        vec![Primitive::Triangle(Triangle::new(a, b, c))],
    );
    Ok(())
}

fn parse_mesh(line: &mut Line<'_>, groups: &mut Groups, load: MeshLoader<'_>) -> Result<()> {
    let name = line.word("mesh file name")?;
    let scale = line.positive("mesh scale")?;
    let offset = line.vec3("mesh offset")?;
    let (key, material) = parse_material(line)?;
    line.finish()?;
    let source = load(name).map_err(|e| line.error(e))?;
    let triangles = parse_obj(&source).map_err(|e| line.error(format!("{name}: {e}")))?;
    if triangles.is_empty() {
        return Err(line.error(format!("{name} contains no triangles")));
    }
    let place = |p: Vec3| p * scale + offset;
    let prims = triangles
        .iter()
        .map(|t| Primitive::Triangle(Triangle::new(place(t.a), place(t.b), place(t.c))))
        .collect();
    groups.add(key, material, prims);
    Ok(())
}

impl SceneFile {
    /// Parses a scene that does not use `mesh` directives.
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_with(text, &mut |name: &str| {
            Err(format!("cannot load `{name}`: no file access here"))
        })
    }

    /// Parses a scene, calling `load` to fetch the text of each mesh file.
    pub fn parse_with(text: &str, load: MeshLoader<'_>) -> Result<Self> {
        let mut sky = Sky::uniform(1.0);
        let mut camera: Option<CameraSpec> = None;
        let mut image: Option<ImageSpec> = None;
        let mut groups = Groups::default();
        let mut last_line = 1;
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            last_line = number;
            let mut line = Line {
                number,
                words: raw.split('#').next().unwrap_or("").split_whitespace(),
            };
            let Some(directive) = line.words.next() else {
                continue;
            };
            match directive {
                "sky" => sky = parse_sky(&mut line)?,
                "camera" => {
                    if camera.is_some() {
                        return Err(line.error("`camera` is defined twice"));
                    }
                    camera = Some(parse_camera(&mut line)?);
                }
                "image" => image = Some(parse_image(&mut line)?),
                "floor" => parse_floor(&mut line, &mut groups)?,
                "sphere" => parse_sphere(&mut line, &mut groups)?,
                "triangle" => parse_triangle(&mut line, &mut groups)?,
                "mesh" => parse_mesh(&mut line, &mut groups, load)?,
                other => return Err(line.error(format!("unknown directive `{other}`"))),
            }
        }
        let camera = camera.ok_or_else(|| PrismError::Parse {
            line: last_line,
            reason: "missing `camera` directive".to_owned(),
        })?;
        let mut scene = Scene::new(sky);
        for (material, prims) in groups.groups {
            scene.add_group(material, prims);
        }
        Ok(Self {
            scene,
            camera,
            image,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Ray;
    use crate::render::{render, RenderSettings};

    const SAMPLE: &str = "# test scene\nsky 0.9 0.3\ncamera 0 2 6  0 1 0  40\nimage 64 36 8\n\
floor 2 0.8 0.05\nsphere 0 1 0 1 glass SF11\nsphere 2 0.5 0 0.5 mirror 0.9\n\
triangle 0 0 0  1 0 0  0 1 0  diffuse 0.5\n";

    const CUBE: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
f 1 2 3 4\nf 5 8 7 6\nf 1 5 6 2\nf 2 6 7 3\nf 3 7 8 4\nf 4 8 5 1\n";

    fn error_line(text: &str) -> Option<usize> {
        if let Err(PrismError::Parse { line, .. }) = SceneFile::parse(text) {
            Some(line)
        } else {
            None
        }
    }

    #[test]
    fn parses_every_directive() {
        let file = SceneFile::parse(SAMPLE).expect("valid scene");
        assert_eq!(file.scene.primitive_count(), 35);
        assert_eq!(
            file.scene.sky,
            Sky {
                horizon: 0.9,
                zenith: 0.3
            }
        );
        assert_eq!(file.camera.eye, Vec3::new(0.0, 2.0, 6.0));
        assert_eq!(file.camera.target, Vec3::new(0.0, 1.0, 0.0));
        assert!((file.camera.fov_degrees - 40.0).abs() < 1e-12);
        assert_eq!(
            file.image,
            Some(ImageSpec {
                width: 64,
                height: 36,
                samples: 8
            })
        );
    }

    #[test]
    fn defaults_without_sky_or_image() {
        let file = SceneFile::parse("camera 0 1 5 0 0 0 40\n").expect("valid scene");
        assert_eq!(file.scene.sky, Sky::uniform(1.0));
        assert!(file.image.is_none());
        assert_eq!(file.scene.primitive_count(), 0);
    }

    #[test]
    fn rejects_bad_files_with_line_numbers() {
        let cam = "camera 0 1 5 0 0 0 40\n";
        assert_eq!(error_line(""), Some(1));
        assert_eq!(error_line("sphere 0 0 0 1 diffuse 0.5\n"), Some(1));
        assert_eq!(error_line(&format!("{cam}bogus 1\n")), Some(2));
        assert_eq!(error_line(&format!("{cam}{cam}")), Some(2));
        assert_eq!(error_line("camera 0 1 5 0 0 0 40 99\n"), Some(1));
        assert_eq!(error_line("camera 0 1 x 0 0 0 40\n"), Some(1));
        assert_eq!(error_line("camera 0 1 5\n"), Some(1));
        assert_eq!(error_line("camera 0 1 5 0 0 0 0\n"), Some(1));
        assert_eq!(error_line("camera 0 1 5 0 0 0 180\n"), Some(1));
        assert_eq!(error_line("camera 0 5 0 0 0 0 40\n"), Some(1));
        assert_eq!(error_line("camera 0 1 5 0 1 5 40\n"), Some(1));
        assert_eq!(
            error_line(&format!("{cam}sphere 0 0 0 1 glass unobtainium\n")),
            Some(2)
        );
        assert_eq!(
            error_line(&format!("{cam}sphere 0 0 0 1 diffuse 1.5\n")),
            Some(2)
        );
        assert_eq!(
            error_line(&format!("{cam}sphere 0 0 0 1 plastic 1\n")),
            Some(2)
        );
        assert_eq!(
            error_line(&format!("{cam}sphere 0 0 0 -1 diffuse 0.5\n")),
            Some(2)
        );
        assert_eq!(
            error_line(&format!("{cam}sphere 0 0 0 1 diffuse\n")),
            Some(2)
        );
        assert_eq!(error_line(&format!("{cam}floor 100 0.5 0.5\n")), Some(2));
        assert_eq!(error_line(&format!("{cam}image 0 10 4\n")), Some(2));
        assert_eq!(error_line(&format!("{cam}image 10 10 0\n")), Some(2));
        assert_eq!(error_line(&format!("{cam}sky -1 1\n")), Some(2));
    }

    #[test]
    fn meshes_are_loaded_scaled_and_placed() {
        let mut loader = |name: &str| {
            if name == "cube.obj" {
                Ok(CUBE.to_owned())
            } else {
                Err(format!("no file {name}"))
            }
        };
        let text = "camera 0 1 5 0 0 0 40\nmesh cube.obj 2 0 0 0 glass N-BK7\n";
        let file = SceneFile::parse_with(text, &mut loader).expect("valid scene");
        assert_eq!(file.scene.primitive_count(), 12);
        let ray = Ray::new(Vec3::new(0.7, 0.3, 10.0), Vec3::new(0.0, 0.0, -1.0));
        let (hit, _) = file
            .scene
            .intersect(&ray, 1e-9, f64::INFINITY)
            .expect("ray hits the cube");
        assert!((hit.t - 8.0).abs() < 1e-9, "t = {}", hit.t);
    }

    #[test]
    fn mesh_errors_are_reported() {
        let text = "camera 0 1 5 0 0 0 40\nmesh a.obj 1 0 0 0 diffuse 0.5\n";
        assert_eq!(error_line(text), Some(2));

        let mut empty = |_: &str| Ok("v 0 0 0\n".to_owned());
        assert!(SceneFile::parse_with(text, &mut empty).is_err());

        let mut broken = |_: &str| Ok("v 0 0 0\nf 1 2 3\n".to_owned());
        assert!(SceneFile::parse_with(text, &mut broken).is_err());
    }

    #[test]
    fn equal_materials_share_a_group() {
        let text =
            "camera 0 1 5 0 0 0 40\nsphere 0 0 0 1 diffuse 0.5\nsphere 3 0 0 1 diffuse 0.5\n";
        let file = SceneFile::parse(text).expect("valid scene");
        assert_eq!(file.scene.primitive_count(), 2);
    }

    #[test]
    fn parsed_scenes_render() {
        let text = "sky 1 1\ncamera 0 0 5 0 0 0 40\nsphere 0 0 0 1 mirror 0.5\n";
        let file = SceneFile::parse(text).expect("valid scene");
        let settings = RenderSettings {
            width: 6,
            height: 6,
            samples: 4,
            max_depth: 4,
            seed: 1,
        };
        let camera = file.camera.camera(1.0);
        let mean = render(&file.scene, &camera, &settings).mean();
        assert!(mean.x.is_finite() && mean.x > 0.1);
    }

    #[test]
    fn demo_example_matches_the_builtin_scene() {
        let file = SceneFile::parse(include_str!("../../../examples/demo.scene")).expect("valid");
        let (builtin, _) = crate::demo::demo_scene(16.0 / 9.0);
        assert_eq!(file.scene.primitive_count(), builtin.primitive_count());
        assert_eq!(
            file.image,
            Some(ImageSpec {
                width: 640,
                height: 360,
                samples: 64
            })
        );
    }

    #[test]
    fn gem_example_parses() {
        let obj = include_str!("../../../examples/octahedron.obj");
        let mut loader = |name: &str| {
            if name == "octahedron.obj" {
                Ok(obj.to_owned())
            } else {
                Err(format!("no file {name}"))
            }
        };
        let file = SceneFile::parse_with(include_str!("../../../examples/gem.scene"), &mut loader)
            .expect("valid");
        assert_eq!(file.scene.primitive_count(), 16 * 16 * 2 + 8);
    }
}
