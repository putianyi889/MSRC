"""Independent v2 compatibility fixtures using only Python's standard library.

The originals are synthetic framing examples, not player recordings. Existing
fixtures are historical compatibility data; do not regenerate for encoder tuning.
"""

import lzma
from pathlib import Path
import struct
import zlib


def planes(values, width):
    return bytes((v >> shift) & 255 for shift in range(0, width, 8) for v in values)


def zigzag16(value):
    return ((value << 1) ^ (value >> 15)) & 65535


def evf3():
    header = bytes.fromhex("03 c0 00 02 02 00 01 10 00 00 00 01 00 00 00")
    header += b"s\0p\0r\0u\0start\0end\0CN\0uuid\0\x80"
    original = bytearray(header)
    types, times, xs, ys = [], [], [], []
    pt = px = py = 0
    for i in range(128):
        code, t, x, y = 2 if i % 17 == 0 else 1, i * 10, (40 + i * 3) % 512, (20 + i * 5) % 512
        original += bytes([code]) + t.to_bytes(3, "big") + struct.pack(">HH", x, y)
        types.append(code)
        times.append((t - pt) & 0xffffff)
        xs.append(zigzag16(x - px))
        ys.append(zigzag16(y - py))
        pt, px, py = t, x, y
    body = bytes(types) + planes(times, 24) + planes(xs, 16) + planes(ys, 16)
    return bytes(original) + b"\xff", header + body + b"\xff", 4, len(header), len(body), 128


def evf4():
    header = bytes.fromhex("04 c0 00 02 02 00 01 10 00 00 00 01")
    header += struct.pack(">I2sQQ", 1234, b"CN", 0, 1234000)
    header += b"s\0p\0r\0u\0\0\0\x80\0\0"
    original = bytearray(header)
    types, times, xs, ys, pauses = [], [], [], [], []
    for i in range(128):
        if i and i % 17 == 0:
            types.append(255)
            pauses.append(300 + i)
            original += b"\xff" + struct.pack(">H", pauses[-1])
        if i and i % 31 == 0:
            types.append(81)
            times.append(3)
            original += b"\x51\x03"
        code, dt = 2 if i % 17 == 0 else 1, i % 9
        dx, dy = (20, 40) if i == 0 else (i % 11 - 5, 3 - i % 7)
        original += struct.pack(">BBhh", code, dt, dx, dy)
        types.append(code)
        times.append(dt)
        xs.append(zigzag16(dx))
        ys.append(zigzag16(dy))
    body = bytes(types) + planes(times, 8) + planes(xs, 16) + planes(ys, 16) + planes(pauses, 16)
    footer = b"\0\0\x05check"
    return bytes(original) + footer, header + body + footer, 5, len(header), len(body), len(types)


def generate():
    destination = Path(__file__).with_name("fixtures")
    for name, make in (("evf3", evf3), ("evf4", evf4)):
        original, filtered, kind, offset, length, count = make()
        assert len(original) == len(filtered)
        descriptor = b"\x01" + struct.pack("<IBQQI", 1, kind, offset, length, count)
        payload = descriptor + lzma.compress(filtered, format=lzma.FORMAT_ALONE, preset=6)
        header = struct.pack("<4sHHHHQQII", b"MSRC", 1, 1, 2, 0, len(original), len(payload),
                             zlib.crc32(original), zlib.crc32(payload))
        archive = header + struct.pack("<I", zlib.crc32(header)) + payload
        for suffix, data in (("hex", archive), ("original.hex", original)):
            path = destination / f"lzma-v2-{name}.{suffix}"
            path.write_text("\n".join(data[i:i + 24].hex(" ") for i in range(0, len(data), 24)) + "\n",
                            encoding="ascii")
        print(f"{name}: {len(original)} -> {len(archive)} bytes")


if __name__ == "__main__":
    generate()
