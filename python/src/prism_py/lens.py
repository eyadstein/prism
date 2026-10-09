"""An independent paraxial lens calculator.

It re-implements the Sellmeier glass model and the paraxial ray trace in Python, separately
from the Rust engine, so the tests can check that both agree.
"""

import math
from dataclasses import dataclass

D_LINE_NM = 587.56

GLASSES: dict[str, tuple[tuple[float, ...], tuple[float, ...]]] = {
    "N-BK7": (
        (1.03961212, 0.231792344, 1.01046945),
        (0.00600069867, 0.0200179144, 103.560653),
    ),
    "FUSED-SILICA": (
        (0.6961663, 0.4079426, 0.8974794),
        (0.00467914826, 0.0135120631, 97.9340025),
    ),
    "F2": (
        (1.34533359, 0.209073176, 0.937357162),
        (0.00997743871, 0.0470450767, 111.886764),
    ),
    "SF11": (
        (1.73759695, 0.313747346, 1.89878101),
        (0.013188707, 0.0623068142, 155.23629),
    ),
    "SAPPHIRE": (
        (1.4313493, 0.65054713, 5.3414021),
        (0.0052799261, 0.0142382647, 325.01783),
    ),
    "DIAMOND": ((0.3306, 4.3356), (0.030625, 0.011236)),
    "WATER": (
        (0.5684027565, 0.1726177391, 0.02086189578, 0.1130748688),
        (0.005101829712, 0.01821153936, 0.02620722293, 10.69792721),
    ),
}


class LensError(ValueError):
    """Raised for prescriptions that cannot be parsed."""


@dataclass(frozen=True)
class Surface:
    """One refracting surface and the medium behind it."""

    radius: float
    thickness: float
    glass: str | None
    semi_aperture: float

    @property
    def curvature(self) -> float:
        """One over the radius, or zero for a flat surface."""
        return 0.0 if self.radius == 0.0 else 1.0 / self.radius


@dataclass(frozen=True)
class Paraxial:
    """First-order lens properties."""

    efl: float
    bfd: float


def refractive_index(glass: str | None, nm: float) -> float:
    """Sellmeier refractive index at `nm` nanometres; `None` means air."""
    if glass is None:
        return 1.0
    b_terms, c_terms = GLASSES[glass.upper()]
    wavelength_sq = (nm / 1000.0) ** 2
    total = sum(
        b * wavelength_sq / (wavelength_sq - c)
        for b, c in zip(b_terms, c_terms, strict=True)
    )
    return math.sqrt(1.0 + total)


def parse_prescription(text: str) -> list[Surface]:
    """Parses one surface per line: `radius thickness glass [aperture]`."""
    surfaces: list[Surface] = []
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        columns = line.split()
        if not 3 <= len(columns) <= 4:
            raise LensError(f"line {number}: expected `radius thickness glass [aperture]`")
        try:
            radius = 0.0 if columns[0].lower() == "flat" else float(columns[0])
            thickness = float(columns[1])
            aperture = float(columns[3]) if len(columns) == 4 else math.inf
        except ValueError as error:
            raise LensError(f"line {number}: {error}") from error
        glass = None if columns[2].lower() == "air" else columns[2].upper()
        if glass is not None and glass not in GLASSES:
            raise LensError(f"line {number}: unknown glass `{columns[2]}`")
        surfaces.append(Surface(radius, thickness, glass, aperture))
    if not surfaces:
        raise LensError("a lens needs at least one surface")
    return surfaces


def paraxial(surfaces: list[Surface], nm: float = D_LINE_NM) -> Paraxial | None:
    """Effective focal length and back focal distance; `None` for an afocal system."""
    height = 1.0
    slope = 0.0
    index = 1.0
    last_height = height
    for surface in surfaces:
        next_index = refractive_index(surface.glass, nm)
        slope = (index * slope - height * surface.curvature * (next_index - index)) / next_index
        last_height = height
        height += surface.thickness * slope
        index = next_index
    if abs(slope) < 1e-15:
        return None
    return Paraxial(efl=-1.0 / slope, bfd=-last_height / slope)