"""Check distribution contents, including regressions caused by packager ignores."""

from pathlib import Path
import sys
import tarfile
import zipfile

kind, directory = sys.argv[1:]
patterns = {"wheel": "*.whl", "sdist": "*.tar.gz", "npm": "*.tgz"}
artifacts = sorted(Path(directory).glob(patterns[kind]))
assert artifacts, f"No {kind} artifacts found in {directory}"
for artifact in artifacts:
    if kind == "wheel":
        with zipfile.ZipFile(artifact) as archive:
            names = archive.namelist()
        assert all(name.startswith(("msrc/", "msrc-")) for name in names), names
        assert "msrc/_native.pyi" in names
        assert "msrc/py.typed" in names
        assert any(name.endswith((".pyd", ".so")) for name in names)
        assert any(name.endswith("/licenses/THIRD_PARTY_NOTICES.md") for name in names)
    else:
        with tarfile.open(artifact) as archive:
            # Strip the conventional package/version root directory.
            names = [name.partition("/")[2] for name in archive.getnames()]
        assert "THIRD_PARTY_NOTICES.md" in names
        assert all(not ({"node_modules", ".tools", ".venv", "target", "dist", "__pycache__"}
                        & set(Path(name).parts)) for name in names)
        if kind == "npm":
            for target in ("nodejs", "web"):
                for file in ("msrc.js", "msrc.d.ts", "msrc_bg.wasm"):
                    assert f"{target}/{file}" in names, (artifact, target, file)
        else:
            for file in ("Cargo.toml", "Cargo.lock", "pyproject.toml", "src/lib.rs",
                         "bindings/python/src/lib.rs", "python/msrc/_native.pyi"):
                assert file in names, (artifact, file)
    print(f"Checked {artifact.name}: {len(names)} entries, {artifact.stat().st_size} bytes")
