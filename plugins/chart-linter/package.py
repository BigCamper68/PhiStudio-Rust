#!/usr/bin/env python3
"""Build the portable plugin and create a deterministic installable ZIP."""
import argparse
import json
import shutil
import subprocess
import zipfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true", help="Package an already built module")
    arguments = parser.parse_args()
    package = Path(__file__).resolve().parent
    repository = package.parent.parent
    if not arguments.skip_build:
        subprocess.run([
            "cargo", "build", "--manifest-path", str(package / "Cargo.toml"),
            "--target", "wasm32-unknown-unknown", "--release", "--locked",
        ], cwd=repository, check=True)
    module = package / "target/wasm32-unknown-unknown/release/phistudio_chart_linter.wasm"
    shutil.copyfile(module, package / "plugin.wasm")
    manifest = json.loads((package / "manifest.json").read_text())
    destination = repository / "artifacts/plugins" / f"chart-linter-{manifest['version']}.zip"
    destination.parent.mkdir(parents=True, exist_ok=True)
    entries = {name: package / name for name in [
        "manifest.json", "plugin.wasm", "README.md", "REFERENCE.md", "Cargo.toml", "Cargo.lock", "package.py",
    ]}
    entries["LICENSE"] = repository / "LICENSE"
    entries.update({str(path.relative_to(package)): path for path in (package / "src").rglob("*.rs")})
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, path in sorted(entries.items()):
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())
    print(destination)


if __name__ == "__main__":
    main()
