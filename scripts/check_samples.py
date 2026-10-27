"""Measure real replay samples with the installed MSRC binding, without editing them."""

import argparse
import csv
import hashlib
from pathlib import Path
import sys
from time import perf_counter

ROOT = Path(__file__).resolve().parents[1]
CONTROL_FILES = {"README.md", ".gitignore", "samples.csv"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=ROOT / "tests/samples")
    parser.add_argument("--report", type=Path, default=ROOT / "target/sample-report.csv")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if not directory.is_dir():
        parser.error(f"Sample directory does not exist: {directory}")
    files = sorted(
        path for path in directory.rglob("*")
        if path.is_file() and not path.is_symlink() and path.name not in CONTROL_FILES
        and not any(part.startswith(".") for part in path.relative_to(directory).parts)
    )
    if not files:
        parser.error(f"No samples found. Add original replay files to {directory}")
    report = args.report.resolve()
    if any(path.resolve() == report for path in files):
        parser.error("Report path overlaps a sample; choose a path outside the sample directory")

    import msrc

    fields = ["path", "sha256", "original_bytes", "archive_bytes", "ratio",
              "saved_percent", "compress_ms", "decompress_ms", "status", "error"]
    report.parent.mkdir(parents=True, exist_ok=True)
    failed = 0
    with report.open("w", newline="", encoding="utf-8") as destination:
        writer = csv.DictWriter(destination, fieldnames=fields)
        writer.writeheader()
        for path in files:
            row = {"path": path.relative_to(directory).as_posix()}
            try:
                original = path.read_bytes()
                row.update(sha256=hashlib.sha256(original).hexdigest(), original_bytes=len(original))
                start = perf_counter()
                archive = msrc.compress(original)
                row["compress_ms"] = round((perf_counter() - start) * 1000, 3)
                start = perf_counter()
                restored = msrc.decompress(archive, max_output_size=len(original))
                row["decompress_ms"] = round((perf_counter() - start) * 1000, 3)
                row["archive_bytes"] = len(archive)
                if original:
                    ratio = len(archive) / len(original)
                    row.update(ratio=round(ratio, 6), saved_percent=round((1 - ratio) * 100, 3))
                if restored != original:
                    raise ValueError("Restored bytes differ from the original")
                row["status"] = "OK"
            except (OSError, ValueError, MemoryError, OverflowError) as error:
                row.update(status="ERROR", error=str(error))
                failed += 1
            writer.writerow(row)
            print(f'{row["status"]}: {row["path"]}', flush=True)
            # Avoid retaining the previous sample's buffers while reading the next.
            original = archive = restored = None
    print(f"{len(files) - failed}/{len(files)} passed; report: {report}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
