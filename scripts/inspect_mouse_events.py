"""Read-only research scanner; see docs/research/mouse-events/README.md.

Recognizes AVF's start heuristic, MVF 0.97/2006/2007, RMV 1/2,
EVF 0.3/0.4 and EVFS 0.0. Older MVF layouts are deliberately not guessed.
This is not a general replay validator, a codec, or a semantic event converter.
No third-party packages are needed. Samples and per-file reports stay local.
"""

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
AVF_CODES = {1, 3, 5, 9, 11, 17, 21, 33, 65, 145, 193}

# Field layouts and the MVF permutation are based on ms-toollib and
# minesweeper-rawvf, pinned and attributed in the research README.
#
# MIT License
# Copyright (c) 2021 eee555
# Copyright (c) 2021 thefinerminer
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.


class Reader:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, size):
        end = self.pos + size
        if size < 0 or end > len(self.data):
            raise ValueError(f"Truncated field at byte {self.pos}, need {size}")
        value = self.data[self.pos:end]
        self.pos = end
        return value

    def uint(self, size=1):
        return int.from_bytes(self.take(size), "big")

    def sint(self, size=2):
        return int.from_bytes(self.take(size), "big", signed=True)

    def cstring(self):
        end = self.data.find(b"\0", self.pos)
        if end < 0:
            raise ValueError(f"Unterminated string at byte {self.pos}")
        return self.take(end - self.pos + 1)


class Events:
    def __init__(self, family, size):
        self.counts = Counter(replays=1, replay_bytes=size)
        self.codes = Counter()
        self.family = family
        self.previous = None

    def other(self, code, size):
        self.codes[str(code)] += 1
        self.counts["other_events"] += 1
        self.counts["other_event_bytes"] += size

    def mouse(self, code, size, t, x, y, move):
        self.codes[str(code)] += 1
        self.counts["mouse_events"] += 1
        self.counts["mouse_bytes"] += size
        self.counts["moves"] += int(move)
        if self.previous is not None:
            pt, px, py = self.previous
            dt, dx, dy = t - pt, x - px, y - py
            self.counts["mouse_pairs"] += 1
            self.counts["backwards_time"] += int(dt < 0)
            self.counts["same_position"] += int(dx == dy == 0)
            self.counts["dt_0_255"] += int(0 <= dt <= 255)
            self.counts["dxy_signed4"] += int(-8 <= dx <= 7 and -8 <= dy <= 7)
            self.counts["dt8_dxy4"] += int(
                0 <= dt <= 255 and -8 <= dx <= 7 and -8 <= dy <= 7
            )
        self.previous = t, x, y


def avf(data):
    r = Reader(data)
    version = r.uint()
    r.take(4)
    level = r.uint()
    mines = {3: 10, 4: 40, 5: 99}.get(level)
    if level == 6:
        r.take(2)
        mines = r.uint(2)
    if mines is None:
        raise ValueError("Unknown AVF level")
    r.take(2 * mines)
    # Same synchronization rule as both upstream parsers; not a magic signature.
    header = re.search(rb"\[[0-3]\|[^\]]*B\d+T[^\]]*\]", data[r.pos:])
    if not header:
        raise ValueError("AVF textual header not found")
    r.take(header.end())
    while True:
        b = r.take(1)[0]
        if b == 1 and r.pos >= 3 and data[r.pos - 2] <= 1:
            r.pos -= 3
            break
    start = r.pos
    result = Events("avf", len(data))
    result.counts[f"header_byte_{version:02x}"] += 1
    while True:
        e = r.take(8)
        seconds = (e[6] << 8) | e[2]
        if seconds == 0:
            result.other("terminator", 8)
            break
        if e[0] not in AVF_CODES:
            raise ValueError(f"Unknown AVF event {e[0]} at byte {r.pos - 8}")
        result.mouse(e[0], 8, (seconds - 1) * 1000 + e[4] * 10,
                     (e[1] << 8) | e[3], (e[5] << 8) | e[7], e[0] == 1)
    return result, start, r.pos


def rmv(data):
    r = Reader(data)
    if r.take(4) != b"*rmv":
        raise ValueError("Missing RMV signature")
    version = r.uint(2)
    if version not in (1, 2):
        raise ValueError(f"Unsupported RMV version {version}")
    if version == 2:
        r.take(2)
    r.take(4)  # file_size is retained, not trusted for bounds
    sizes = [r.uint(2) for _ in range(6)]
    video_size, checksum_size = r.uint(4), r.uint(2)
    if video_size < 4:
        raise ValueError("RMV vid_size is too small for a termination event")
    # v1: result, version, player, board, preflags, properties.
    # v2: version, player, board, preflags, properties, extensions.
    r.take(sum(sizes))
    start = r.pos
    end = start + video_size
    result = Events(f"rmv-{version}", len(data))
    t = x = y = 0
    while r.pos < end:
        code = r.uint()
        if 1 <= code <= 7:
            t = r.uint(3)
            r.take(1)  # preserve nFlags in the source; no normalization
            x, y = (r.uint(2), r.uint(2)) if version == 1 else (r.sint(), r.sint())
            result.mouse(code, 9, t, x, y, code == 1)
        elif code == 28 and version == 2 and result.previous is not None:
            t += r.uint()
            packed = r.uint()
            dx, dy = packed >> 4, packed & 15
            x += dx if dx < 8 else dx - 16
            y += dy if dy < 8 else dy - 16
            result.mouse(code, 3, t, x, y, True)
        elif 9 <= code <= 14 or 18 <= code <= 27:
            r.take(2)
            result.other(code, 3)
        elif code == 0 and version == 1:
            r.take(4)
            result.other(code, 5)
        elif 15 <= code <= 17:
            r.take(3)
            result.other(code, 4)
            # The draft specifies three time bytes after the code, but all
            # current v1 samples have five more bytes within vid_size. Keep
            # them opaque; do not misread them as more events or a checksum.
            tail_size = end - r.pos
            result.counts["opaque_video_tail_bytes"] += tail_size
            result.counts[f"opaque_video_tail_size_{tail_size}"] += 1
            r.take(tail_size)
            break
        else:
            raise ValueError(f"Unknown RMV event {code} at byte {r.pos - 1}")
    if r.pos != end or not 15 <= code <= 17:
        raise ValueError("RMV termination does not match vid_size")
    r.take(checksum_size)
    if r.pos != len(data):
        raise ValueError("Unexpected RMV trailing data")
    return result, start, end


def mvf(data):
    r = Reader(data)
    if data[:2] != b"\x11M" or len(data) <= 27 or data[27] not in b"567":
        raise ValueError("Unsupported MVF header/version (no heuristic fallback)")
    old = data[27] == ord("5")
    r.take(74 if old else 71)
    r.take(12)  # date, level, mode, score
    r.take(11 if old else 1)  # old statistics (10 bytes), questionmarks
    r.take(2)  # width, height
    r.take(2 * r.uint(2))  # mine coordinates
    r.take(r.uint())  # player name
    seed = r.uint(2)
    n1, n2 = math.sqrt(seed), math.sqrt(seed + 1000.0)
    n3, n4 = math.sqrt(n1 + 1000.0), math.sqrt(n2 + 1000.0)
    values = [math.cos(n3 + 1000.0), math.sin(math.sqrt(n2)), math.cos(n3),
              math.sin(math.sqrt(n1) + 1000.0), math.cos(n4)]
    if not old:
        values.append(math.sin(n4))
    digits = "".join(f"{round(abs(value * 100000000)):08d}" for value in values)
    width = 5 if old else 6
    if len(digits) != width * 8:
        raise ValueError("Unexpected MVF permutation digit count")
    permutation = sorted(range(width * 8), key=lambda bit: (digits[bit], bit))
    count = r.uint(3)  # a record count, despite an upstream comment saying bytes
    start = r.pos
    result = Events("mvf-0.97" if old else "mvf-2006-2007", len(data))
    previous_buttons = None
    for _ in range(count):
        raw = r.take(width)
        # Positions within each byte are numbered from the least significant bit.
        logical = sum(((raw[p // 8] >> (p % 8)) & 1) << i
                      for i, p in enumerate(permutation))
        buttons = logical & 7  # R, M, L; snapshots, not event codes
        if old:
            y, x = (logical >> 3) & 511, (logical >> 12) & 511
            t = ((logical >> 28) & 1023) * 1000 + ((logical >> 21) & 127) * 10
            residual = logical >> 38
        else:
            y, x = ((logical >> 3) & 2047) - 32, ((logical >> 14) & 2047) - 32
            t = (logical >> 25) & ((1 << 22) - 1)
            residual = logical >> 47
        changed_position = result.previous is not None and result.previous[1:] != (x, y)
        result.mouse(f"buttons_{buttons}", width, t, x, y, changed_position)
        result.counts["nonzero_residual_records"] += int(residual != 0)
        result.counts["button_state_changes"] += int(
            previous_buttons is not None and buttons != previous_buttons
        )
        previous_buttons = buttons
    result.counts["trailing_bytes"] = len(data) - r.pos
    return result, start, r.pos


def evf(data):
    r = Reader(data)
    version = r.uint()
    if version not in (3, 4):
        raise ValueError(f"Unsupported EVF version {version}")
    flags = r.uint()
    r.take(1)
    rows, cols = r.uint(), r.uint()
    r.take(7)  # mine count, cell size, mode, 3BV
    if version == 3:
        r.take(3)
        for _ in range(8):  # software, IDs, timestamps, country, UUID
            r.cstring()
    else:
        r.take(22)  # time_ms, country, two u64 timestamps
        r.cstring()
        if flags & 8:
            r.cstring()
            r.cstring()
        for _ in range(3):
            r.cstring()
        r.take(r.uint(2))
    r.take((rows * cols + 7) // 8)
    if version == 4:
        for _ in range(r.uint(2)):
            r.cstring()
    start = r.pos
    result = Events(f"evf-0.{version}", len(data))
    t = x = y = 0
    while True:
        code = r.uint()
        if code == 0 or (version == 3 and code == 255):
            result.other("terminator", 1)
            break
        if 1 <= code <= 12:
            if version == 3:
                t, x, y = r.uint(3), r.uint(2), r.uint(2)
            else:
                t += r.uint()
                # The first coordinate delta is relative to (0, 0).
                x, y = x + r.sint(), y + r.sint()
            result.mouse(code, 8 if version == 3 else 6, t, x, y, code == 1)
        elif version == 4 and code == 255:
            t += r.uint(2)
            result.other(code, 3)
        elif version == 4 and code in (81, 82, 83, 99):
            t += r.uint()
            result.other(code, 2)
        else:
            # EVF 0.4 board fields have conflicting width/type in the spec.
            # Reject rather than guess record boundaries, including metric events.
            raise ValueError(f"Unimplemented EVF event {code} at byte {r.pos - 1}")
    end = r.pos
    r.take((32 if code == 0 else 0) if version == 3 else r.uint(2))
    if r.pos != len(data):
        raise ValueError("Unexpected EVF trailing data")
    return result, start, end


def scan(data, extension):
    if extension != ".evfs":
        result, start, end = {".avf": avf, ".mvf": mvf, ".rmv": rmv, ".evf": evf}[extension](data)
        result.counts["event_bytes"] = end - start
        return [result]
    r = Reader(data)
    if r.uint() != 0:
        raise ValueError("Unsupported EVFS version")
    checksum_size = r.uint(2)
    results = []
    while r.pos < len(data):
        r.cstring()
        results.extend(scan(r.take(r.uint(4)), ".evf"))
        r.take(checksum_size)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=ROOT / "tests/samples")
    parser.add_argument("--report", type=Path, default=ROOT / "target/mouse-event-summary.json")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if not directory.is_dir():
        parser.error("Sample directory does not exist")
    report = args.report.resolve()
    if report.is_relative_to(directory):
        parser.error("Report must be outside the sample directory")
    groups = defaultdict(Counter)
    codes = defaultdict(Counter)
    files, failures = [], []
    for path in sorted(directory.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        extension = path.suffix.lower()
        if extension not in (".avf", ".mvf", ".rmv", ".evf", ".evfs"):
            continue
        row = {"path": path.relative_to(directory).as_posix()}
        try:
            data = path.read_bytes()
            results = scan(data, extension)
            row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), replays=len(results))
            for result in results:
                groups[result.family].update(result.counts)
                codes[result.family].update(result.codes)
            files.append(row)
        except (ValueError, OSError) as error:
            failures.append(dict(row, error=str(error)))
    if not files and not failures:
        parser.error("No supported replay files found")
    summary = {name: dict(counts, codes=dict(codes[name])) for name, counts in sorted(groups.items())}
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(dict(groups=summary, files=files, failures=failures),
                                 ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(files=len(files), failures=len(failures), groups=summary), indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
