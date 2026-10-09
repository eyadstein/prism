"""Denoiser benchmark.

Renders a scene with the Rust command line tool at several sample counts, with and without
the wavelet denoiser, and scores every image against a high-sample reference render.
"""

import argparse
import os
import subprocess
import sys
import tempfile
import time
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

from prism_py.metrics import psnr, ssim
from prism_py.png import read_png


@dataclass(frozen=True)
class Row:
    """One benchmark result."""

    samples: int
    denoised: bool
    psnr: float
    ssim: float
    seconds: float


def repo_root() -> Path:
    """The repository root, found relative to this file."""
    return Path(__file__).resolve().parents[3]


def binary_path(root: Path) -> Path:
    """Where `cargo build --release -p prism-cli` puts the command line tool."""
    name = "prism.exe" if os.name == "nt" else "prism"
    return root / "target" / "release" / name


def render(
    binary: Path,
    scene: Path,
    out: Path,
    *,
    width: int,
    height: int,
    samples: int,
    denoise: bool,
) -> float:
    """Runs `prism render` and returns the wall-clock seconds it took."""
    command = [
        str(binary),
        "render",
        str(scene),
        "--out",
        str(out),
        "--width",
        str(width),
        "--height",
        str(height),
        "--samples",
        str(samples),
    ]
    if denoise:
        command.append("--denoise")
    started = time.perf_counter()
    subprocess.run(command, check=True, capture_output=True)
    return time.perf_counter() - started


def benchmark(
    binary: Path,
    scene: Path,
    *,
    width: int = 320,
    height: int = 180,
    samples: Sequence[int] = (4, 16, 64),
    reference: int = 512,
) -> list[Row]:
    """Scores noisy and denoised renders at each sample count against a reference."""
    rows: list[Row] = []
    with tempfile.TemporaryDirectory() as folder:
        work = Path(folder)
        reference_path = work / "reference.png"
        render(
            binary,
            scene,
            reference_path,
            width=width,
            height=height,
            samples=reference,
            denoise=False,
        )
        truth = read_png(reference_path)
        for count in samples:
            for denoise in (False, True):
                out = work / f"{count}-{int(denoise)}.png"
                seconds = render(
                    binary,
                    scene,
                    out,
                    width=width,
                    height=height,
                    samples=count,
                    denoise=denoise,
                )
                image = read_png(out)
                rows.append(Row(count, denoise, psnr(truth, image), ssim(truth, image), seconds))
    return rows


def format_table(rows: Sequence[Row]) -> str:
    """A Markdown table of benchmark rows."""
    lines = [
        "| Samples | Denoised | PSNR (dB) | SSIM | Time (s) |",
        "|---|---|---|---|---|",
    ]
    for row in rows:
        label = "yes" if row.denoised else "no"
        lines.append(
            f"| {row.samples} | {label} | {row.psnr:.2f} | {row.ssim:.3f} | {row.seconds:.2f} |"
        )
    return "\n".join(lines)


def render_document(
    rows: Sequence[Row], scene_name: str, width: int, height: int, reference: int
) -> str:
    """The text of docs/benchmarks.md."""
    return (
        "# Denoiser benchmark\n\n"
        f"Scene `{scene_name}` at {width} x {height}. Every image is scored against a render of "
        f"the same scene at {reference} samples per pixel. PSNR and SSIM are computed on the "
        "8-bit sRGB images, and higher is better. Regenerate with `prism-bench`.\n\n"
        f"{format_table(rows)}\n"
    )


def build_parser() -> argparse.ArgumentParser:
    """The command line parser of `prism-bench`."""
    parser = argparse.ArgumentParser(
        prog="prism-bench", description="Benchmark the Prism wavelet denoiser."
    )
    parser.add_argument("--scene", default="examples/demo.scene", help="scene file to render")
    parser.add_argument("--out", default="docs/benchmarks.md", help="Markdown file to write")
    parser.add_argument("--width", type=int, default=320)
    parser.add_argument("--height", type=int, default=180)
    parser.add_argument("--samples", type=int, nargs="+", default=[4, 16, 64])
    parser.add_argument("--reference", type=int, default=512)
    parser.add_argument("--no-build", action="store_true", help="do not run cargo build first")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Entry point of `prism-bench`."""
    parser = build_parser()
    args = parser.parse_args(argv)
    if min(args.width, args.height, args.reference, *args.samples) < 1:
        parser.error("width, height and sample counts must be at least 1")
    root = repo_root()
    scene = Path(args.scene)
    if not scene.is_absolute():
        scene = root / scene
    out = Path(args.out)
    if not out.is_absolute():
        out = root / out
    if not args.no_build:
        subprocess.run(["cargo", "build", "--release", "-p", "prism-cli"], cwd=root, check=True)
    binary = binary_path(root)
    if not binary.exists():
        print(f"cannot find {binary}; run: cargo build --release -p prism-cli", file=sys.stderr)
        return 1
    rows = benchmark(
        binary,
        scene,
        width=args.width,
        height=args.height,
        samples=tuple(args.samples),
        reference=args.reference,
    )
    text = render_document(rows, scene.name, args.width, args.height, args.reference)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())