import math

import numpy as np
import pytest

from prism_py.metrics import mse, psnr, ssim


def texture(seed: int = 0, size: int = 32) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return rng.integers(0, 256, size=(size, size, 3)).astype(np.uint8)


def noisy(image: np.ndarray, sigma: float, seed: int = 1) -> np.ndarray:
    rng = np.random.default_rng(seed)
    out = image.astype(np.float64) + rng.normal(0.0, sigma, size=image.shape)
    return np.clip(out, 0, 255).astype(np.uint8)


def test_mse_and_psnr_of_a_constant_offset() -> None:
    a = np.zeros((8, 8, 3), dtype=np.uint8)
    b = np.full((8, 8, 3), 10, dtype=np.uint8)
    assert mse(a, b) == pytest.approx(100.0)
    assert psnr(a, b) == pytest.approx(10 * math.log10(255**2 / 100))
    assert psnr(a, b, peak=1.0) == pytest.approx(10 * math.log10(1 / 100))
    assert psnr(a, a) == math.inf


def test_shape_mismatch_is_an_error() -> None:
    with pytest.raises(ValueError, match="shape"):
        mse(np.zeros((4, 4, 3)), np.zeros((4, 5, 3)))


def test_identical_images_have_unit_ssim() -> None:
    image = texture()
    assert ssim(image, image) == pytest.approx(1.0, abs=1e-9)


def test_more_noise_means_lower_ssim() -> None:
    base = texture()
    mild = ssim(base, noisy(base, 10.0))
    heavy = ssim(base, noisy(base, 40.0))
    assert 1.0 > mild > heavy


def test_ssim_is_symmetric() -> None:
    a = texture(0)
    b = texture(1)
    assert ssim(a, b) == pytest.approx(ssim(b, a))


def test_ssim_accepts_grayscale_and_floats() -> None:
    a = texture(0)
    b = noisy(a, 20.0)
    assert ssim(a[:, :, 0], a[:, :, 0]) == pytest.approx(1.0, abs=1e-9)
    assert ssim(a.astype(np.float64), b.astype(np.float64)) == pytest.approx(ssim(a, b))


def test_ssim_needs_room_for_the_window() -> None:
    small = texture(size=5)
    with pytest.raises(ValueError, match="window"):
        ssim(small, small)