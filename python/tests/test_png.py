import struct
import zlib
from pathlib import Path

import numpy as np
import pytest

from prism_py.png import SIGNATURE, PngError, decode_png, encode_png, read_png, write_png

ROOT = Path(__file__).resolve().parents[2]


def random_image(height: int, width: int, seed: int = 0) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return rng.integers(0, 256, size=(height, width, 3)).astype(np.uint8)


def chunk(kind: bytes, body: bytes) -> bytes:
    crc = zlib.crc32(kind + body) & 0xFFFFFFFF
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)


def paeth(a: int, b: int, c: int) -> int:
    estimate = a + b - c
    pa, pb, pc = abs(estimate - a), abs(estimate - b), abs(estimate - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def filtered_png(image: np.ndarray, filters: list[int]) -> bytes:
    height, width, _ = image.shape
    stride = width * 3
    raw = bytearray()
    previous = [0] * stride
    for y in range(height):
        line = [int(v) for v in image[y].reshape(-1)]
        kind = filters[y % len(filters)]
        out = []
        for i, x in enumerate(line):
            a = line[i - 3] if i >= 3 else 0
            b = previous[i]
            c = previous[i - 3] if i >= 3 else 0
            predictor = [0, a, b, (a + b) // 2, paeth(a, b, c)][kind]
            out.append((x - predictor) & 255)
        raw.append(kind)
        raw.extend(out)
        previous = line
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        SIGNATURE
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw)))
        + chunk(b"IEND", b"")
    )


def stored_zlib(raw: bytes) -> bytes:
    out = bytearray(b"\x78\x01")
    blocks = [raw[i : i + 65535] for i in range(0, len(raw), 65535)] or [b""]
    for index, block in enumerate(blocks):
        out.append(1 if index == len(blocks) - 1 else 0)
        out += struct.pack("<HH", len(block), len(block) ^ 0xFFFF)
        out += block
    out += struct.pack(">I", zlib.adler32(raw))
    return bytes(out)


def test_round_trip() -> None:
    image = random_image(7, 11)
    assert np.array_equal(decode_png(encode_png(image)), image)


def test_signature_and_dimensions() -> None:
    data = encode_png(random_image(3, 5))
    assert data.startswith(SIGNATURE)
    assert struct.unpack(">II", data[16:24]) == (5, 3)


def test_empty_image() -> None:
    empty = np.zeros((0, 0, 3), dtype=np.uint8)
    assert decode_png(encode_png(empty)).shape == (0, 0, 3)


def test_every_filter_type_is_undone() -> None:
    image = random_image(10, 9, seed=3)
    assert np.array_equal(decode_png(filtered_png(image, [0, 1, 2, 3, 4])), image)


def test_decodes_uncompressed_deflate_like_the_rust_engine_writes() -> None:
    image = random_image(200, 200, seed=5)
    raw = b"".join(b"\x00" + image[y].tobytes() for y in range(200))
    assert len(raw) > 65535
    header = struct.pack(">IIBBBBB", 200, 200, 8, 2, 0, 0, 0)
    idat = chunk(b"IDAT", stored_zlib(raw))
    data = SIGNATURE + chunk(b"IHDR", header) + idat + chunk(b"IEND", b"")
    assert np.array_equal(decode_png(data), image)


def test_rejects_bad_input() -> None:
    good = encode_png(random_image(2, 2))
    with pytest.raises(PngError, match="not a PNG"):
        decode_png(b"hello world, this is not a png")
    corrupted = bytearray(good)
    corrupted[20] ^= 0xFF
    with pytest.raises(PngError, match="checksum"):
        decode_png(bytes(corrupted))
    with pytest.raises(PngError, match="truncated"):
        decode_png(good[:-5])
    rgba = struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0)
    unsupported = SIGNATURE + chunk(b"IHDR", rgba) + chunk(b"IEND", b"")
    with pytest.raises(PngError, match="only 8-bit RGB"):
        decode_png(unsupported)


def test_encoder_rejects_wrong_arrays() -> None:
    with pytest.raises(PngError):
        encode_png(np.zeros((2, 2), dtype=np.uint8))
    with pytest.raises(PngError):
        encode_png(np.zeros((2, 2, 3), dtype=np.float64))


def test_files_round_trip(tmp_path: Path) -> None:
    image = random_image(4, 6, seed=9)
    path = tmp_path / "x.png"
    write_png(path, image)
    assert np.array_equal(read_png(path), image)


@pytest.mark.skipif(not (ROOT / "docs" / "demo.png").exists(), reason="demo render missing")
def test_reads_the_png_written_by_the_rust_engine() -> None:
    image = read_png(ROOT / "docs" / "demo.png")
    assert image.shape == (360, 640, 3)
    assert image.mean() > 20