"""Reading and writing 8-bit RGB PNG files with only the standard library and NumPy."""

import struct
import zlib
from pathlib import Path

import numpy as np
import numpy.typing as npt

SIGNATURE = b"\x89PNG\r\n\x1a\n"

Image = npt.NDArray[np.uint8]


class PngError(ValueError):
    """Raised when data is not a supported PNG file."""


def _chunk(kind: bytes, body: bytes) -> bytes:
    crc = zlib.crc32(kind + body) & 0xFFFFFFFF
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)


def _read_chunks(data: bytes) -> list[tuple[bytes, bytes]]:
    if data[:8] != SIGNATURE:
        raise PngError("not a PNG file")
    chunks: list[tuple[bytes, bytes]] = []
    pos = 8
    while pos < len(data):
        if pos + 12 > len(data):
            raise PngError("truncated chunk")
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        end = pos + 8 + length
        if end + 4 > len(data):
            raise PngError("truncated chunk")
        body = data[pos + 8 : end]
        (crc,) = struct.unpack(">I", data[end : end + 4])
        if zlib.crc32(kind + body) & 0xFFFFFFFF != crc:
            raise PngError(f"bad checksum in {kind.decode('latin-1')} chunk")
        chunks.append((kind, body))
        pos = end + 4
        if kind == b"IEND":
            break
    return chunks


def _unfilter(filter_type: int, line: list[int], previous: list[int]) -> list[int]:
    """Undoes one PNG scanline filter; neighbours are 3 bytes apart for 8-bit RGB."""
    if filter_type == 0:
        return list(line)
    if filter_type not in (1, 2, 3, 4):
        raise PngError(f"unknown filter type {filter_type}")
    current = [0] * len(line)
    for i, x in enumerate(line):
        a = current[i - 3] if i >= 3 else 0
        b = previous[i]
        c = previous[i - 3] if i >= 3 else 0
        if filter_type == 1:
            value = x + a
        elif filter_type == 2:
            value = x + b
        elif filter_type == 3:
            value = x + (a + b) // 2
        else:
            estimate = a + b - c
            pa, pb, pc = abs(estimate - a), abs(estimate - b), abs(estimate - c)
            if pa <= pb and pa <= pc:
                predictor = a
            elif pb <= pc:
                predictor = b
            else:
                predictor = c
            value = x + predictor
        current[i] = value & 255
    return current


def decode_png(data: bytes) -> Image:
    """Decodes an 8-bit RGB, non-interlaced PNG into a (height, width, 3) uint8 array."""
    chunks = _read_chunks(data)
    if not chunks or chunks[0][0] != b"IHDR" or len(chunks[0][1]) != 13:
        raise PngError("missing or malformed IHDR chunk")
    width, height, depth, colour, compression, filtering, interlace = struct.unpack(
        ">IIBBBBB", chunks[0][1]
    )
    if (depth, colour, compression, filtering, interlace) != (8, 2, 0, 0, 0):
        raise PngError("only 8-bit RGB, non-interlaced PNG files are supported")
    try:
        raw = zlib.decompress(b"".join(body for kind, body in chunks if kind == b"IDAT"))
    except zlib.error as error:
        raise PngError(f"cannot decompress pixel data: {error}") from error
    stride = width * 3
    if len(raw) != (stride + 1) * height:
        raise PngError("pixel data has the wrong size")
    rows: list[list[int]] = []
    previous = [0] * stride
    for y in range(height):
        start = y * (stride + 1)
        line = list(raw[start + 1 : start + 1 + stride])
        current = _unfilter(raw[start], line, previous)
        rows.append(current)
        previous = current
    return np.array(rows, dtype=np.uint8).reshape(height, width, 3)


def encode_png(image: Image) -> bytes:
    """Encodes a (height, width, 3) uint8 array as a PNG file."""
    if image.dtype != np.uint8 or image.ndim != 3 or image.shape[2] != 3:
        raise PngError("expected an array of shape (height, width, 3) and dtype uint8")
    height, width = image.shape[0], image.shape[1]
    rows = b"".join(b"\x00" + image[y].tobytes() for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        SIGNATURE
        + _chunk(b"IHDR", header)
        + _chunk(b"IDAT", zlib.compress(rows))
        + _chunk(b"IEND", b"")
    )


def read_png(path: Path | str) -> Image:
    """Reads a PNG file."""
    return decode_png(Path(path).read_bytes())


def write_png(path: Path | str, image: Image) -> None:
    """Writes a PNG file."""
    Path(path).write_bytes(encode_png(image))