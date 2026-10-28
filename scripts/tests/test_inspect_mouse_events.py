"""Synthetic framing checks; no private replay samples are embedded here."""

from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from inspect_mouse_events import avf, evf, mvf, rmv, scan


def evf4(events, checksum=b""):
    header = bytes.fromhex("04 c0 00 02 02 00 01 10 00 00 00 01")
    header += bytes(22) + b"s\0p\0r\0u\0" + b"\0\0\x80\0\0"
    return header + events + b"\0" + len(checksum).to_bytes(2, "big") + checksum


def rmv_file(version, events):
    header = b"*rmv" + version.to_bytes(2, "big")
    if version == 2:
        header += bytes(2)
    # No metadata needed for a record-layout test. The scanner is not a validator.
    return header + bytes(4 + 12) + len(events).to_bytes(4, "big") + bytes(2) + events


def mvf_file(old, raw):
    header = bytearray(74 if old else 71)
    header[:2] = b"\x11M"
    header[27] = ord("5" if old else "7")
    header += bytes(12 + (11 if old else 1))
    header += b"\x02\x02\0\0\0"  # board size, no mines, empty name
    return bytes(header) + b"\x30\x39\0\0\x01" + raw


class MouseEventLayoutTests(unittest.TestCase):
    def test_avf_interleaving_and_alias_code(self):
        header = bytes.fromhex("34 00 00 00 00 03") + bytes(20) + b"[0|B1T1.00]"
        data = header + bytes.fromhex("91 00 01 23 45 01 00 67") + bytes(8)
        events, start, end = avf(data)
        self.assertEqual(events.previous, (690, 0x23, 0x167))
        self.assertEqual(events.codes["145"], 1)
        self.assertEqual((start, end), (len(header), len(data)))

    def test_evf3_absolute_ms_and_optional_checksum(self):
        header = bytes.fromhex("03 c0 00 02 02 00 01 10 00 00 00 01 00 00 00")
        header += b"s\0p\0r\0u\0start\0end\0CN\0uuid\0\x80"
        data = header + bytes.fromhex("02 00 01 23 00 10 00 20 ff")
        events, _, end = evf(data)
        self.assertEqual(events.previous, (0x123, 16, 32))
        self.assertEqual(end, len(data))

    def test_evf4_pause_and_game_state_advance_shared_time(self):
        events = bytes.fromhex("02 00 00 0a 00 14 ff 03 e8 51 07 03 05 ff fe 00 03")
        result, _, _ = evf(evf4(events, b"check"))
        self.assertEqual(result.previous, (1012, 8, 23))
        self.assertEqual(result.counts["mouse_events"], 2)
        self.assertEqual(result.codes["255"], 1)
        self.assertEqual(result.codes["81"], 1)

    def test_rmv2_signed_nibbles_follow_mouse_across_board_record(self):
        events = bytes.fromhex("01 00 00 64 01 ff f0 00 20 09 00 00 1c 05 f8 10 00 00 69")
        result, _, _ = rmv(rmv_file(2, events))
        self.assertEqual(result.previous, (105, -17, 24))
        self.assertEqual(result.counts["mouse_bytes"], 12)

    def test_rmv_legacy_tail_is_retained_within_video_boundary(self):
        result, start, end = rmv(rmv_file(1, bytes.fromhex("10 00 03 f3 00 00 00 03 f3")))
        self.assertEqual(end - start, 9)
        self.assertEqual(result.counts["opaque_video_tail_bytes"], 5)

    def test_mvf_golden_records_keep_residual_bits(self):
        result, _, _ = mvf(mvf_file(True, bytes.fromhex("66 38 84 22 7d")))
        self.assertEqual(result.previous, (12340, 321, 123))
        self.assertEqual(result.codes["buttons_3"], 1)
        self.assertEqual(result.counts["nonzero_residual_records"], 1)
        result, _, _ = mvf(mvf_file(False, bytes.fromhex("ec 28 20 ad 02 15")))
        self.assertEqual(result.previous, (123456, 777, -4))
        self.assertEqual(result.codes["buttons_5"], 1)
        self.assertEqual(result.counts["nonzero_residual_records"], 1)

    def test_evfs_has_independent_evf_coordinate_baselines(self):
        a = evf4(bytes.fromhex("02 00 00 0a 00 14"))
        b = evf4(bytes.fromhex("02 00 00 1e 00 28"))
        data = b"\0\0\x02a\0" + struct.pack(">I", len(a)) + a + b"cs"
        data += b"b\0" + struct.pack(">I", len(b)) + b + b"cs"
        results = scan(data, ".evfs")
        self.assertEqual([r.previous for r in results], [(0, 10, 20), (0, 30, 40)])

    def test_unknown_and_truncated_records_are_rejected(self):
        for data in (evf4(b"\x64\0\0\0"), evf4(b"\x02\0\0"), evf4(b"")[:-1]):
            with self.subTest(data=data), self.assertRaises(ValueError):
                evf(data)
        with self.assertRaises(ValueError):
            scan(b"\0\0\x02a\0\0\0\0\x20short", ".evfs")
        with self.assertRaises(ValueError):
            mvf(mvf_file(True, bytes.fromhex("66 38 84 22")))


if __name__ == "__main__":
    unittest.main()
