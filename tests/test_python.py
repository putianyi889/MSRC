import lzma
from pathlib import Path
import struct
import unittest
import zlib

import msrc

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = b"MSRC interoperability fixture\n" + bytes(range(256)) * 4


class CodecTests(unittest.TestCase):
    def test_roundtrip_and_bytes_return_type(self):
        for data in (b"", b"\0\xff", ORIGINAL, b"event\n" * 2000):
            with self.subTest(size=len(data)):
                archive = msrc.compress(data)
                self.assertIsInstance(archive, bytes)
                self.assertEqual(msrc.decompress(archive), data)
                self.assertEqual(msrc.inspect(archive).original_size, len(data))

    def test_rust_encoder_with_stdlib_decoder(self):
        for level in range(10):
            payload = msrc.compress(ORIGINAL, level=level)[40:]
            reader = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE)
            self.assertEqual(reader.decompress(payload), ORIGINAL)
            self.assertTrue(reader.eof)
            self.assertEqual(reader.unused_data, b"")

    def test_independent_fixture(self):
        archive = bytes.fromhex((ROOT / "tests/fixtures/lzma-v1.hex").read_text())
        self.assertEqual(msrc.decompress(archive), ORIGINAL)
        info = msrc.inspect(archive)
        self.assertEqual((info.container_version, info.algorithm_id, info.algorithm_version), (1, 1, 1))
        with self.assertRaises(AttributeError):
            info.original_size = 0

    def test_corruption_and_limits(self):
        archive = msrc.compress(ORIGINAL)
        for bad in (b"", archive[:-1], archive + b"\0"):
            with self.assertRaises(ValueError):
                msrc.decompress(bad)
        with self.assertRaises(ValueError):
            msrc.decompress(archive, max_output_size=len(ORIGINAL) - 1)
        with self.assertRaises(ValueError):
            msrc.decompress(archive, max_memory_kib=0)
        with self.assertRaises(ValueError):
            msrc.compress(b"", level=10)
        for level in (-1, 1.5):
            with self.assertRaises((TypeError, OverflowError)):
                msrc.compress(b"", level=level)

    def test_stdlib_encoder_at_runtime(self):
        # Construct the documented envelope independently of the Rust library.
        payload = lzma.compress(ORIGINAL, format=lzma.FORMAT_ALONE, preset=0)
        header = struct.pack("<4sHHHHQQII", b"MSRC", 1, 1, 1, 0, len(ORIGINAL), len(payload),
                             zlib.crc32(ORIGINAL), zlib.crc32(payload))
        archive = header + struct.pack("<I", zlib.crc32(header)) + payload
        self.assertEqual(msrc.decompress(archive), ORIGINAL)


if __name__ == "__main__":
    unittest.main()
