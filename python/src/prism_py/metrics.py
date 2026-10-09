"""Image quality metrics: MSE, PSNR and SSIM."""

import math
from typing import Any

import numpy as np
import numpy.typing as npt

Pixels = npt.NDArray[Any]


def _as_float(a: Pixels, b: Pixels) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
    if a.shape != b.shape:
        raise ValueError(f"shape mismatch: {a.shape} and {b.shape}")
    return a.astype(np.float64), b.astype(np.float64)


def mse(a: Pixels, b: Pixels) -> float:
    """Mean squared error between two images of the same shape."""
    x, y = _as_float(a, b)
    diff = x - y
    return float(np.mean(diff * diff))


def psnr(a: Pixels, b: Pixels, peak: float = 255.0) -> float:
    """Peak signal-to-noise ratio in decibels; infinite for identical images."""
    error = mse(a, b)
    if error == 0.0:
        return math.inf
    return 10.0 * math.log10(peak * peak / error)


def _box_mean(x: npt.NDArray[np.float64], size: int) -> npt.NDArray[np.float64]:
    """Mean over every size by size window that fits inside `x`, via an integral image."""
    padded = np.zeros((x.shape[0] + 1, x.shape[1] + 1), dtype=np.float64)
    padded[1:, 1:] = x.cumsum(axis=0).cumsum(axis=1)
    rows = x.shape[0] - size + 1
    cols = x.shape[1] - size + 1
    total = (
        padded[size : size + rows, size : size + cols]
        - padded[:rows, size : size + cols]
        - padded[size : size + rows, :cols]
        + padded[:rows, :cols]
    )
    return total / float(size * size)


def ssim(a: Pixels, b: Pixels, window: int = 7, peak: float = 255.0) -> float:
    """Mean structural similarity with a uniform window, averaged over channels."""
    x, y = _as_float(a, b)
    if x.ndim == 2:
        x = x[:, :, np.newaxis]
        y = y[:, :, np.newaxis]
    if x.ndim != 3:
        raise ValueError("expected a 2D or 3D array")
    if min(x.shape[0], x.shape[1]) < window:
        raise ValueError("image is smaller than the SSIM window")
    c1 = (0.01 * peak) ** 2
    c2 = (0.03 * peak) ** 2
    scores: list[float] = []
    for channel in range(x.shape[2]):
        p = x[:, :, channel]
        q = y[:, :, channel]
        mean_p = _box_mean(p, window)
        mean_q = _box_mean(q, window)
        var_p = _box_mean(p * p, window) - mean_p * mean_p
        var_q = _box_mean(q * q, window) - mean_q * mean_q
        cov = _box_mean(p * q, window) - mean_p * mean_q
        numerator = (2.0 * mean_p * mean_q + c1) * (2.0 * cov + c2)
        denominator = (mean_p**2 + mean_q**2 + c1) * (var_p + var_q + c2)
        scores.append(float(np.mean(numerator / denominator)))
    return float(np.mean(scores))