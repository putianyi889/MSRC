"""Reproduce the compatibility fixture using only Python's standard library.

The input is synthetic opaque binary data, not a valid Minesweeper replay.
This file is deliberately independent of the Rust implementation.
"""

import lzma
from pathlib import Path
import struct
import zlib

original = b"MSRC interoperability fixture\n" + bytes(range(256)) * 4
payload = lzma.compress(original, format=lzma.FORMAT_ALONE, preset=6)
header = struct.pack(
    "<4sHHHHQQII", b"MSRC", 1, 1, 1, 0, len(original), len(payload),
    zlib.crc32(original), zlib.crc32(payload),
)
archive = header + struct.pack("<I", zlib.crc32(header)) + payload
destination = Path(__file__).parent / "fixtures" / "lzma-v1.hex"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(archive.hex(" ") + "\n", encoding="ascii")
