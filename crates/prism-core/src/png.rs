//! Minimal PNG encoder: 8-bit RGB, uncompressed deflate blocks, no dependencies.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const MAX_STORED: usize = 65_535;

const fn make_table() -> [u32; 256] {
    let mut table = [0_u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if (c & 1) == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = make_table();

fn crc_update(mut c: u32, data: &[u8]) -> u32 {
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c
}

/// CRC-32 (IEEE) of `data`, as used by PNG chunks.
pub fn crc32(data: &[u8]) -> u32 {
    crc_update(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
}

/// Adler-32 checksum of `data`, as used by zlib streams.
pub fn adler32(data: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78_u8, 0x01];
    if raw.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    let mut blocks = raw.chunks(MAX_STORED).peekable();
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        let len = block.len() as u16;
        out.push(u8::from(last));
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn write_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc_update(crc_update(0xFFFF_FFFF, &kind), data) ^ 0xFFFF_FFFF;
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encodes tightly packed 8-bit RGB pixels (row-major) as a PNG file.
///
/// # Panics
///
/// Panics when `rgb.len()` is not `width * height * 3`.
pub fn encode_rgb8(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row = width as usize * 3;
    assert_eq!(
        rgb.len(),
        row * height as usize,
        "pixel buffer size mismatch"
    );
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    if row > 0 {
        for line in rgb.chunks(row) {
            raw.push(0);
            raw.extend_from_slice(line);
        }
    }
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut out = SIGNATURE.to_vec();
    write_chunk(&mut out, *b"IHDR", &header);
    write_chunk(&mut out, *b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut out, *b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_checksums() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn small_image_structure() {
        let png = encode_rgb8(2, 2, &[10; 12]);
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &2_u32.to_be_bytes());
        assert_eq!(&png[20..24], &2_u32.to_be_bytes());
        let iend = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
        assert!(png.ends_with(&iend));
    }

    #[test]
    fn large_images_use_several_stored_blocks() {
        let png = encode_rgb8(200, 200, &vec![0_u8; 120_000]);
        assert_eq!(png.len(), 120_273);
    }

    #[test]
    fn empty_image_is_valid() {
        let png = encode_rgb8(0, 0, &[]);
        assert!(png.starts_with(&SIGNATURE));
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
    }

    #[test]
    #[should_panic(expected = "pixel buffer size mismatch")]
    fn wrong_buffer_size_panics() {
        let _ = encode_rgb8(2, 2, &[0; 5]);
    }
}
