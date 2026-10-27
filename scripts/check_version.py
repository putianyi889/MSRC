"""Check package versions and require an exact matching version on tag builds."""

import json
import os
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
# Works on Python 3.10 as well, without a TOML parser dependency.
cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
workspace = cargo.split("[workspace.package]", 1)[1].split("\n[", 1)[0]
version = re.search(r'^version\s*=\s*"([^"]+)"', workspace, re.MULTILINE).group(1)
npm_version = json.loads((root / "package.json").read_text(encoding="utf-8"))["version"]
if version != npm_version:
    raise SystemExit(f"Version mismatch: Cargo={version}, npm={npm_version}")
ref = os.environ.get("GITHUB_REF", "")
if ref.startswith("refs/tags/") and ref != f"refs/tags/v{version}":
    raise SystemExit(f"Tag must be v{version}, got {ref}")
print(f"Package versions agree: {version}")
