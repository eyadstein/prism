import math
import re
import subprocess

import pytest

from prism_py.bench import binary_path, repo_root
from prism_py.lens import (
    D_LINE_NM,
    GLASSES,
    LensError,
    paraxial,
    parse_prescription,
    refractive_index,
)

ROOT = repo_root()
BINARY = binary_path(ROOT)
EXAMPLES = ["plano-convex.lens", "doublet.lens", "singlet.lens"]


def test_d_line_indices_match_datasheets() -> None:
    table = {
        "N-BK7": (1.5168, 1e-3),
        "FUSED-SILICA": (1.4585, 2e-3),
        "F2": (1.6200, 3e-3),
        "SF11": (1.7847, 3e-3),
        "SAPPHIRE": (1.7682, 3e-3),
        "DIAMOND": (2.4175, 5e-3),
        "WATER": (1.3330, 2e-3),
    }
    assert set(table) == set(GLASSES)
    for name, (expected, tolerance) in table.items():
        assert abs(refractive_index(name, D_LINE_NM) - expected) < tolerance, name


def test_blue_bends_more_than_red_and_air_is_one() -> None:
    for name in GLASSES:
        assert refractive_index(name, 450.0) > refractive_index(name, 650.0), name
    assert refractive_index(None, 550.0) == 1.0
    assert refractive_index("n-bk7", 550.0) == refractive_index("N-BK7", 550.0)


def test_parses_comments_flat_air_and_apertures() -> None:
    surfaces = parse_prescription("# demo\n51.68 5.0 N-BK7 12.5   # note\n\nflat 90 air\n")
    assert len(surfaces) == 2
    assert surfaces[0].glass == "N-BK7"
    assert surfaces[0].semi_aperture == 12.5
    assert surfaces[1].glass is None
    assert surfaces[1].radius == 0.0
    assert surfaces[1].curvature == 0.0
    assert math.isinf(surfaces[1].semi_aperture)


@pytest.mark.parametrize(
    "text",
    [
        "",
        "# only a comment",
        "1 2",
        "1 2 air 3 4",
        "x 2 air",
        "1 2 unobtainium",
        "1 abc air",
        "1 2 air x",
    ],
)
def test_rejects_bad_prescriptions(text: str) -> None:
    with pytest.raises(LensError):
        parse_prescription(text)


def test_errors_name_the_line() -> None:
    with pytest.raises(LensError, match="line 2"):
        parse_prescription("1 2 air\n1 2 unobtainium\n")


def test_plano_convex_matches_closed_form() -> None:
    n = refractive_index("N-BK7", D_LINE_NM)
    result = paraxial(parse_prescription("51.68 5.0 N-BK7\nflat 90 air\n"))
    assert result is not None
    focal = 51.68 / (n - 1.0)
    assert result.efl == pytest.approx(focal, abs=1e-9)
    assert result.bfd == pytest.approx(focal - 5.0 / n, abs=1e-9)


def test_biconvex_matches_the_lensmakers_equation() -> None:
    n = refractive_index("N-BK7", D_LINE_NM)
    r1, r2, d = 100.0, -100.0, 5.0
    result = paraxial(parse_prescription(f"{r1} {d} N-BK7\n{r2} 50 air\n"))
    assert result is not None
    power = (n - 1.0) * (1.0 / r1 - 1.0 / r2 + (n - 1.0) * d / (n * r1 * r2))
    assert result.efl == pytest.approx(1.0 / power, abs=1e-9)
    assert result.bfd == pytest.approx(result.efl * (1.0 - (n - 1.0) * d / (n * r1)), abs=1e-9)


def test_flat_plate_is_afocal() -> None:
    assert paraxial(parse_prescription("flat 5 N-BK7\nflat 10 air\n")) is None


def test_blue_focuses_closer_than_red() -> None:
    surfaces = parse_prescription("51.68 5.0 N-BK7\nflat 90 air\n")
    blue = paraxial(surfaces, 450.0)
    red = paraxial(surfaces, 650.0)
    assert blue is not None
    assert red is not None
    assert blue.bfd < red.bfd


def test_matches_the_numbers_printed_by_the_rust_tool() -> None:
    plano = paraxial(parse_prescription((ROOT / "examples" / "plano-convex.lens").read_text()))
    doublet = paraxial(parse_prescription((ROOT / "examples" / "doublet.lens").read_text()))
    assert plano is not None
    assert doublet is not None
    assert plano.efl == pytest.approx(100.000, abs=1e-3)
    assert plano.bfd == pytest.approx(96.704, abs=1e-3)
    assert doublet.efl == pytest.approx(99.652, abs=1e-3)
    assert doublet.bfd == pytest.approx(95.188, abs=1e-3)


@pytest.mark.skipif(not BINARY.exists(), reason="build the release binary first")
@pytest.mark.parametrize("name", EXAMPLES)
def test_agrees_with_the_rust_engine(name: str) -> None:
    path = ROOT / "examples" / name
    result = subprocess.run(
        [str(BINARY), "analyze", str(path)], check=True, capture_output=True, text=True
    )
    match = re.search(
        r"effective focal length (-?[0-9.]+), back focal distance (-?[0-9.]+)", result.stdout
    )
    assert match is not None
    expected = paraxial(parse_prescription(path.read_text()))
    assert expected is not None
    assert float(match.group(1)) == pytest.approx(expected.efl, abs=1e-3)
    assert float(match.group(2)) == pytest.approx(expected.bfd, abs=1e-3)