//! The `prism` command line tool.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use prism_core::denoise::{denoise, DenoiseSettings};
use prism_core::designer::{design_doublet, rank_pairs, PairScore};
use prism_core::features::compute as compute_features;
use prism_core::film::Image;
use prism_core::glass::catalog;
use prism_core::lens::{Lens, D_LINE_NM};
use prism_core::lenscam::LensCamera;
use prism_core::math::Vec3;
use prism_core::optimize::{optimize, rms_spot_radius, Outcome, Problem};
use prism_core::render::{render, RenderSettings};
use prism_core::report::lens_report;
use prism_core::scenefile::SceneFile;

const CROWN_THICKNESS_MM: f64 = 4.0;
const FLINT_THICKNESS_MM: f64 = 2.5;

#[derive(Parser)]
#[command(
    name = "prism",
    version,
    about = "Spectral light simulator and lens designer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct RenderArgs {
    /// Path to the scene file.
    scene: String,
    /// Output PNG file.
    #[arg(short, long, default_value = "renders/render.png")]
    out: String,
    /// Image width in pixels (default: from the scene file, else 640).
    #[arg(long)]
    width: Option<usize>,
    /// Image height in pixels (default: from the scene file, else 360).
    #[arg(long)]
    height: Option<usize>,
    /// Spectral samples per pixel (default: from the scene file, else 64).
    #[arg(long)]
    samples: Option<u32>,
    /// Filter the image with the edge-avoiding wavelet denoiser.
    #[arg(long)]
    denoise: bool,
    /// Render through this lens prescription instead of a pinhole camera.
    #[arg(long)]
    lens: Option<String>,
    /// Distance in scene units (metres) at which the lens is focused; used with --lens.
    #[arg(long, default_value_t = 5.0)]
    focus: f64,
    /// Sensor width in millimetres; used with --lens.
    #[arg(long, default_value_t = 36.0)]
    sensor_width: f64,
}

#[derive(Args)]
struct OptimizeArgs {
    /// Path to the lens prescription.
    lens: String,
    /// Required effective focal length; defaults to the current one.
    #[arg(long)]
    target_efl: Option<f64>,
    /// Surfaces whose curvature may change, for example 0,2 (default: all).
    #[arg(long, value_delimiter = ',')]
    vary: Vec<usize>,
    /// Entrance pupil radius.
    #[arg(long, default_value_t = 5.0)]
    pupil: f64,
    /// Field angles in degrees, for example 0,5.
    #[arg(long, value_delimiter = ',', default_value = "0")]
    fields: Vec<f64>,
    /// Maximum number of iterations.
    #[arg(long, default_value_t = 60)]
    iterations: usize,
    /// Write the optimized prescription to this file instead of printing it.
    #[arg(short, long)]
    out: Option<String>,
}

#[derive(Args)]
struct DesignArgs {
    /// Required effective focal length in millimetres.
    #[arg(long, default_value_t = 100.0)]
    efl: f64,
    /// Semi-aperture of every surface in millimetres.
    #[arg(long, default_value_t = 12.5)]
    aperture: f64,
    /// How many of the best glass pairs to list.
    #[arg(long, default_value_t = 8)]
    top: usize,
    /// Largest allowed element power as a multiple of the total power.
    #[arg(long, default_value_t = 4.0)]
    max_power: f64,
    /// Write the designed prescription to this file instead of printing it.
    #[arg(short, long)]
    out: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Render a scene file to a PNG image.
    Render(RenderArgs),
    /// Optimize a lens prescription with damped least squares.
    Optimize(OptimizeArgs),
    /// Search every crown and flint pair for the smallest secondary spectrum and build the doublet.
    Design(DesignArgs),
    /// List the glass catalogue with refractive index, Abbe number and partial dispersion.
    Glasses,
    /// Analyze a lens prescription: focal length, spot sizes, distortion, field curvature, MTF.
    Analyze {
        /// Path to the lens prescription.
        lens: String,
        /// Entrance pupil radius used for the spot diagrams.
        #[arg(long, default_value_t = 5.0)]
        pupil: f64,
        /// Field angle in degrees.
        #[arg(long, default_value_t = 0.0)]
        field: f64,
    },
    /// Render the built-in demo scene to a PNG file.
    Demo {
        /// Output file.
        #[arg(short, long, default_value = "renders/demo.png")]
        out: String,
        /// Image width in pixels.
        #[arg(long, default_value_t = 640)]
        width: usize,
        /// Image height in pixels.
        #[arg(long, default_value_t = 360)]
        height: usize,
        /// Spectral samples per pixel.
        #[arg(long, default_value_t = 64)]
        samples: u32,
    },
    /// Print version and supported wavelength range.
    Info,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Info => {
            println!(
                "prism {} (spectral range {} to {} nm)",
                prism_core::VERSION,
                prism_core::WAVELENGTH_MIN_NM,
                prism_core::WAVELENGTH_MAX_NM
            );
            ExitCode::SUCCESS
        }
        Command::Demo {
            out,
            width,
            height,
            samples,
        } => run_demo(&out, width, height, samples),
        Command::Render(args) => run_render(&args),
        Command::Analyze { lens, pupil, field } => run_analyze(&lens, pupil, field),
        Command::Optimize(args) => run_optimize(&args),
        Command::Design(args) => run_design(&args),
        Command::Glasses => run_glasses(),
    }
}

fn write_file(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = Path::new(path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)
}

fn save(out: &str, image: &Image, width: usize, height: usize, samples: u32) -> ExitCode {
    match write_file(out, &image.to_png()) {
        Ok(()) => {
            println!("wrote {out} ({width}x{height}, {samples} spp)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cannot write {out}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn run_demo(out: &str, width: usize, height: usize, samples: u32) -> ExitCode {
    if width == 0 || height == 0 {
        eprintln!("width and height must be at least 1");
        return ExitCode::FAILURE;
    }
    let (scene, camera) = prism_core::demo::demo_scene(width as f64 / height as f64);
    let settings = RenderSettings {
        width,
        height,
        samples,
        ..RenderSettings::default()
    };
    let image = render(&scene, &camera, &settings);
    save(out, &image, width, height, samples)
}

fn load_lens(path: &str) -> Result<Lens, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    Lens::parse(&text).map_err(|e| format!("{path}: {e}"))
}

fn render_image(
    args: &RenderArgs,
    file: &SceneFile,
    settings: &RenderSettings,
    aspect: f64,
) -> Result<Image, String> {
    if let Some(path) = &args.lens {
        if args.denoise {
            return Err("--denoise cannot be combined with --lens".to_owned());
        }
        let lens = load_lens(path)?;
        let camera = LensCamera::new(
            &lens,
            file.camera.eye,
            file.camera.target,
            Vec3::new(0.0, 1.0, 0.0),
            args.focus,
            args.sensor_width,
            aspect,
        )
        .map_err(|e| format!("lens camera: {e}"))?;
        return Ok(render(&file.scene, &camera, settings));
    }
    let camera = file.camera.camera(aspect);
    let image = render(&file.scene, &camera, settings);
    if args.denoise {
        let features = compute_features(&file.scene, &camera, settings.width, settings.height);
        return denoise(&image, &features, &DenoiseSettings::default())
            .map_err(|e| format!("denoising failed: {e}"));
    }
    Ok(image)
}

#[allow(clippy::cast_precision_loss)]
fn run_render(args: &RenderArgs) -> ExitCode {
    let text = match std::fs::read_to_string(&args.scene) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args.scene);
            return ExitCode::FAILURE;
        }
    };
    let base = Path::new(&args.scene)
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let mut loader = |name: &str| {
        std::fs::read_to_string(base.join(name)).map_err(|e| format!("cannot read {name}: {e}"))
    };
    let file = match SceneFile::parse_with(&text, &mut loader) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("{}: {e}", args.scene);
            return ExitCode::FAILURE;
        }
    };
    let preset = file.image;
    let width = args.width.or(preset.map(|p| p.width)).unwrap_or(640);
    let height = args.height.or(preset.map(|p| p.height)).unwrap_or(360);
    let samples = args.samples.or(preset.map(|p| p.samples)).unwrap_or(64);
    if width == 0 || height == 0 || samples == 0 {
        eprintln!("width, height and samples must be at least 1");
        return ExitCode::FAILURE;
    }
    let settings = RenderSettings {
        width,
        height,
        samples,
        ..RenderSettings::default()
    };
    match render_image(args, &file, &settings, width as f64 / height as f64) {
        Ok(image) => save(&args.out, &image, width, height, samples),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run_analyze(path: &str, pupil: f64, field: f64) -> ExitCode {
    match load_lens(path) {
        Ok(lens) => analyze(&lens, path, pupil, field),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn analyze(lens: &Lens, path: &str, pupil: f64, field: f64) -> ExitCode {
    if let Some(text) = lens_report(lens, path, pupil, field) {
        print!("{text}");
        ExitCode::SUCCESS
    } else {
        eprintln!("{path}: lens has no finite focal length");
        ExitCode::FAILURE
    }
}

fn run_glasses() -> ExitCode {
    println!("{:<14} {:>8} {:>8} {:>8}", "glass", "n_d", "V_d", "P_gF");
    for glass in catalog() {
        println!(
            "{:<14} {:>8.4} {:>8.2} {:>8.4}",
            glass.name,
            glass.index(D_LINE_NM),
            glass.abbe(),
            glass.partial_dispersion()
        );
    }
    ExitCode::SUCCESS
}

fn pair_row(rank: &str, pair: &PairScore, efl: f64) -> String {
    format!(
        "{rank:>4}  {:<9} {:<9} {:>6.1} {:>6.1} {:>10.3} {:>13.1} {:>6.2}",
        pair.crown.name,
        pair.flint.name,
        pair.crown.abbe(),
        pair.flint.abbe(),
        pair.secondary * 100.0,
        pair.focal_shift(efl).abs() * 1000.0,
        pair.power_ratio()
    )
}

fn run_design(args: &DesignArgs) -> ExitCode {
    let ranked = rank_pairs(args.max_power);
    let Some(best) = ranked.first().copied() else {
        eprintln!("no glass pair satisfies --max-power {}", args.max_power);
        return ExitCode::FAILURE;
    };
    println!(
        "glass pairs ranked by secondary spectrum, focal length {} mm:",
        args.efl
    );
    println!("rank  crown     flint        V1     V2  2nd spec %  g-F shift um  power");
    for (index, pair) in ranked.iter().take(args.top).enumerate() {
        println!("{}", pair_row(&(index + 1).to_string(), pair, args.efl));
    }
    if let Some(base) = ranked
        .iter()
        .find(|p| p.crown.name == "N-BK7" && p.flint.name == "F2")
    {
        println!("{}", pair_row("ref", base, args.efl));
    }
    let lens = match design_doublet(
        best.crown,
        best.flint,
        args.efl,
        args.aperture,
        CROWN_THICKNESS_MM,
        FLINT_THICKNESS_MM,
    ) {
        Ok(lens) => lens,
        Err(e) => {
            eprintln!("cannot build the best pair: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "designed cemented doublet: {} (crown) + {} (flint)",
        best.crown.name, best.flint.name
    );
    if let Some(text) = lens_report(&lens, "design", args.aperture * 0.8, 0.0) {
        print!("{text}");
    }
    let text = lens.to_prescription();
    if let Some(path) = &args.out {
        return match write_file(path, text.as_bytes()) {
            Ok(()) => {
                println!("wrote {path}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("cannot write {path}: {e}");
                ExitCode::FAILURE
            }
        };
    }
    print!("{text}");
    ExitCode::SUCCESS
}

fn run_optimize(args: &OptimizeArgs) -> ExitCode {
    let lens = match load_lens(&args.lens) {
        Ok(lens) => lens,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let Some(start) = lens.paraxial(D_LINE_NM) else {
        eprintln!("{}: lens has no finite focal length", args.lens);
        return ExitCode::FAILURE;
    };
    let vary = if args.vary.is_empty() {
        (0..lens.surfaces().len()).collect()
    } else {
        args.vary.clone()
    };
    let mut problem = Problem::new(
        lens.with_image_distance(start.bfd),
        vary,
        args.target_efl.unwrap_or(start.efl),
    );
    problem.pupil_radius = args.pupil;
    problem.fields_deg.clone_from(&args.fields);
    let before_um = rms_spot_radius(&problem.lens, &problem) * 1000.0;
    match optimize(&problem, args.iterations) {
        Ok(outcome) => report(args, &problem, &outcome, start.efl, before_um),
        Err(e) => {
            eprintln!("optimization failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn report(
    args: &OptimizeArgs,
    problem: &Problem,
    outcome: &Outcome,
    start_efl: f64,
    before_um: f64,
) -> ExitCode {
    let end_efl = outcome.lens.paraxial(D_LINE_NM).map_or(f64::NAN, |p| p.efl);
    let after_um = rms_spot_radius(&outcome.lens, problem) * 1000.0;
    println!("polychromatic RMS spot radius, focal length (d line):");
    println!("  before: {before_um:.2} um, EFL {start_efl:.3}");
    println!(
        "  after:  {after_um:.2} um, EFL {end_efl:.3} ({} iterations)",
        outcome.iterations
    );
    let text = outcome.lens.to_prescription();
    if let Some(path) = &args.out {
        return match write_file(path, text.as_bytes()) {
            Ok(()) => {
                println!("wrote {path}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("cannot write {path}: {e}");
                ExitCode::FAILURE
            }
        };
    }
    print!("{text}");
    ExitCode::SUCCESS
}
