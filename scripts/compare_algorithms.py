"""Compare both retained algorithms on private samples, checking exact recovery."""

import argparse
from collections import defaultdict
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
from statistics import median
from time import perf_counter

import msrc

ROOT = Path(__file__).resolve().parents[1]


def environment():
    cpu = platform.processor()
    if os.name == "nt":
        import winreg
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
            cpu = winreg.QueryValueEx(key, "ProcessorNameString")[0].strip()
    return dict(os=platform.platform(), cpu=cpu, logical_cpus=os.cpu_count(),
                python=platform.python_version(), implementation=platform.python_implementation(),
                machine=platform.machine())


def source_fingerprint():
    files = [ROOT / name for name in ("Cargo.toml", "Cargo.lock", "bindings/python/Cargo.toml",
                                     "bindings/python/src/lib.rs")]
    files.extend((ROOT / "src").rglob("*.rs"))
    digest = hashlib.sha256()
    for path in sorted(files):
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes().replace(b"\r\n", b"\n"))
        digest.update(b"\0")
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=ROOT / "tests/samples")
    parser.add_argument("--report", type=Path, default=ROOT / "target/algorithm-comparison.csv")
    parser.add_argument("--summary", type=Path, default=ROOT / "target/algorithm-comparison.json")
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--level", type=int, choices=range(10), default=6)
    args = parser.parse_args()
    directory = args.directory.resolve()
    if not directory.is_dir():
        parser.error("Sample directory does not exist")
    if args.rounds < 1 or args.warmups < 0:
        parser.error("rounds must be positive and warmups nonnegative")
    outputs = [args.report.resolve(), args.summary.resolve()]
    if outputs[0] == outputs[1] or any(p.is_relative_to(directory) for p in outputs):
        parser.error("Report paths must be distinct and outside the sample directory")
    files = sorted(p for p in directory.rglob("*") if p.is_file() and not p.is_symlink()
                   and p.name not in {"README.md", ".gitignore", "samples.csv"}
                   and not any(part.startswith(".") for part in p.relative_to(directory).parts))
    if not files:
        parser.error("No samples found")
    fields = ["path", "sha256", "format", "original_bytes", "v1_bytes", "v2_bytes",
              "saved_vs_v1_percent", "v2_preprocessed", "v1_compress_ms", "v2_compress_ms",
              "v1_decompress_ms", "v2_decompress_ms", "timing_samples_ms", "status", "error"]
    for path in outputs:
        path.parent.mkdir(parents=True, exist_ok=True)
    groups = defaultdict(lambda: defaultdict(int))
    failures = 0
    with outputs[0].open("w", encoding="utf-8", newline="") as destination:
        writer = csv.DictWriter(destination, fieldnames=fields)
        writer.writeheader()
        for index, path in enumerate(files, 1):
            row = {"path": path.relative_to(directory).as_posix()}
            try:
                data = path.read_bytes()
                family = path.suffix.lower().lstrip(".")
                if family == "evf" and data:
                    family += f"-0.{data[0]}"
                if family == "rmv" and data.startswith(b"*rmv") and len(data) >= 6:
                    family += f"-{int.from_bytes(data[4:6], 'big')}"
                row.update(format=family, original_bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
                timings = {f"v{v}_{operation}_ms": [] for v in (1, 2)
                           for operation in ("compress", "decompress")}
                for repeat in range(-args.warmups, args.rounds):
                    # Alternate order to avoid consistently giving one version
                    # the warmer cache. File I/O, inspect and equality are untimed.
                    for version in ((1, 2) if (index + repeat) % 2 else (2, 1)):
                        start = perf_counter()
                        archive = msrc.compress(data, level=args.level, algorithm_version=version)
                        compress_ms = (perf_counter() - start) * 1000
                        info = msrc.inspect(archive)
                        if (info.algorithm_id, info.algorithm_version) != (1, version):
                            raise ValueError("Encoder used the wrong algorithm")
                        row[f"v{version}_bytes"] = len(archive)
                        start = perf_counter()
                        restored = msrc.decompress(archive, max_output_size=len(data))
                        decompress_ms = (perf_counter() - start) * 1000
                        if restored != data:
                            raise ValueError(f"Algorithm {version} did not restore the original bytes")
                        if repeat >= 0:
                            timings[f"v{version}_compress_ms"].append(compress_ms)
                            timings[f"v{version}_decompress_ms"].append(decompress_ms)
                        if version == 2:
                            row["v2_preprocessed"] = int(archive[40] == 1)
                row.update({key: median(samples) for key, samples in timings.items()})
                row["timing_samples_ms"] = json.dumps(timings)
                row["saved_vs_v1_percent"] = 100 * (1 - row["v2_bytes"] / row["v1_bytes"])
                row["status"] = "OK"
                g = groups[family]
                g["files"] += 1
                for key in ("original_bytes", "v1_bytes", "v2_bytes", "v2_preprocessed",
                            "v1_compress_ms", "v2_compress_ms", "v1_decompress_ms", "v2_decompress_ms"):
                    g[key] += row[key]
            except (OSError, ValueError, OverflowError, MemoryError) as error:
                row.update(status="ERROR", error=str(error))
                failures += 1
            writer.writerow(row)
            if index % 50 == 0 or index == len(files):
                print(f"{index}/{len(files)} checked; failures={failures}", flush=True)
    totals = defaultdict(int)
    for g in groups.values():
        for key, value in g.items():
            totals[key] += value
    for g in [*groups.values(), totals]:
        if g.get("v1_bytes"):
            g["saved_vs_v1_percent"] = 100 * (1 - g["v2_bytes"] / g["v1_bytes"])
    summary = dict(package_version=msrc.__version__, source_sha256=source_fingerprint(), environment=environment(),
                   level=args.level, rounds=args.rounds, warmups=args.warmups,
                   timing_statistic="sum of per-file medians", failures=failures,
                   groups=dict(sorted(groups.items())), totals=dict(totals))
    outputs[1].write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
