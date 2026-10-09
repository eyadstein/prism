import math

import pytest

from prism_py.bench import Row, benchmark, binary_path, build_parser, format_table, repo_root

BINARY = binary_path(repo_root())


def test_table_formatting() -> None:
    rows = [
        Row(4, False, 20.123, 0.5, 0.25),
        Row(4, True, 25.0, 0.8, 0.3),
        Row(16, False, math.inf, 1.0, 1.0),
    ]
    lines = format_table(rows).splitlines()
    assert lines[0] == "| Samples | Denoised | PSNR (dB) | SSIM | Time (s) |"
    assert lines[1] == "|---|---|---|---|---|"
    assert lines[2] == "| 4 | no | 20.12 | 0.500 | 0.25 |"
    assert lines[3] == "| 4 | yes | 25.00 | 0.800 | 0.30 |"
    assert lines[4] == "| 16 | no | inf | 1.000 | 1.00 |"


def test_parser_defaults() -> None:
    args = build_parser().parse_args([])
    assert args.samples == [4, 16, 64]
    assert args.reference == 512
    assert (args.width, args.height) == (320, 180)
    assert args.no_build is False


@pytest.mark.skipif(not BINARY.exists(), reason="build the release binary first")
def test_benchmark_runs_end_to_end() -> None:
    scene = repo_root() / "examples" / "demo.scene"
    rows = benchmark(BINARY, scene, width=32, height=18, samples=(2,), reference=8)
    assert [row.denoised for row in rows] == [False, True]
    for row in rows:
        assert row.psnr > 5.0
        assert -1.0 <= row.ssim <= 1.0
        assert row.seconds >= 0.0