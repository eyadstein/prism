//! The `prism` command line tool.

use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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

#[derive(Subcommand)]
enum Command {
    /// Render a scene file to an image.
    Render {
        /// Path to the scene description.
        scene: String,
    },
    /// Optimize a lens prescription.
    Optimize {
        /// Path to the lens prescription.
        lens: String,
    },
    /// Analyze a lens prescription (spot diagram, MTF, distortion).
    Analyze {
        /// Path to the lens prescription.
        lens: String,
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
        Command::Render { scene } => not_yet("render", &scene),
        Command::Optimize { lens } => not_yet("optimize", &lens),
        Command::Analyze { lens } => not_yet("analyze", &lens),
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
