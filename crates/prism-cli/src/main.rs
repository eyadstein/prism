//! The `prism` command line tool.

use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use prism_core::lens::{Lens, D_LINE_NM};
use prism_core::optimize::{optimize, rms_spot_radius, Outcome, Problem};
use prism_core::render::{render, RenderSettings};

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

#[derive(Subcommand)]
enum Command {
    /// Render a scene file to an image.
    Render {
        /// Path to the scene description.
        scene: String,
    },
    /// Optimize a lens prescription with damped least squares.
    Optimize(OptimizeArgs),
    /// Analyze a lens prescription: focal length and RMS spot size at three wavelengths.
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
        Command::Analyze { lens, pupil, field } => run_analyze(&lens, pupil, field),
        Command::Optimize(args) => run_optimize(&args),
        Command::Render { scene } => not_yet("render", &scene),
    }
}

fn not_yet(name: &str, input: &str) -> ExitCode {
    eprintln!("`prism {name} {input}` is not implemented yet");
    ExitCode::from(2)
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

fn load_lens(path: &str) -> Result<Lens, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    Lens::parse(&text).map_err(|e| format!("{path}: {e}"))
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
    let Some(p) = lens.paraxial(D_LINE_NM) else {
        eprintln!("{path}: lens has no finite focal length");
        return ExitCode::FAILURE;
    };
    println!("{path}: {} surfaces", lens.surfaces().len());
    println!(
        "effective focal length {:.3}, back focal distance {:.3} (paraxial, d line)",
        p.efl, p.bfd
    );
    println!("spot diagrams at the paraxial focus: pupil radius {pupil}, field {field} deg");
    let focused = lens.with_image_distance(p.bfd);
    for nm in [450.0, 550.0, 650.0] {
        let text = focused
            .spot_diagram(nm, field, 41, pupil)
            .rms_radius()
            .map_or_else(
                || "every ray was blocked".to_owned(),
                |r| format!("RMS spot radius {:.2} um", r * 1000.0),
            );
        println!("  {nm:.0} nm  {text}");
    }
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
