"""Convert a bounded RGB PPM to a deterministic, lossless PNG portrait."""
import re
import struct
import zlib


def ppm_to_png(ppm):
    match = re.match(rb'P6\n([1-9][0-9]{0,4}) ([1-9][0-9]{0,4})\n255\n', ppm)
    if not match:
        raise ValueError('unsupported PPM portrait header')
    width, height = map(int, match.groups())
    pixels = ppm[match.end():]
    if width * height > 2_000_000 or len(pixels) != width * height * 3:
        raise ValueError('invalid or oversized PPM portrait')
    rows = b''.join(b'\0' + pixels[i:i + width * 3] for i in range(0, len(pixels), width * 3))
    # Stored DEFLATE blocks make the representation reproducible across zlib
    # versions while preserving every decoded RGB pixel from the source image.
    compressed = bytearray(b'\x78\x01')
    for offset in range(0, len(rows), 65535):
        block = rows[offset:offset + 65535]
        compressed.append(1 if offset + len(block) == len(rows) else 0)
        compressed.extend(struct.pack('<HH', len(block), 65535 - len(block)))
        compressed.extend(block)
    compressed.extend(struct.pack('>I', zlib.adler32(rows)))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', bytes(compressed)) + chunk(b'IEND', b''))
