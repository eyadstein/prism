//! The `prism` command line tool.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
        Command::Render { scene } => not_yet("render", &scene),
        Command::Optimize { lens } => not_yet("optimize", &lens),
        Command::Analyze { lens } => not_yet("analyze", &lens),
    }
}

fn not_yet(name: &str, input: &str) -> ExitCode {
    eprintln!("`prism {name} {input}` is not implemented yet");
    ExitCode::from(2)
}
