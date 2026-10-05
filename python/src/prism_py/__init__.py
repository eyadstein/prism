"""Python tooling for the Prism spectral renderer."""

__version__ = "0.1.0"

WAVELENGTH_MIN_NM = 380.0
WAVELENGTH_MAX_NM = 780.0


def visible_range_nm() -> tuple[float, float]:
    """Return the (min, max) wavelength the engine samples, in nanometres."""
    return (WAVELENGTH_MIN_NM, WAVELENGTH_MAX_NM)
