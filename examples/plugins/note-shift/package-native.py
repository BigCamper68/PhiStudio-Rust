#!/usr/bin/env python3
"""Package the current platform's built cdylib without loading its code."""
import json
from pathlib import Path
import shutil
import sys

root = Path(__file__).resolve().parent
if sys.platform == "win32":
    source_name = "phistudio_example_note_shift.dll"
    entry = "plugin.dll"
elif sys.platform == "darwin":
    source_name = "libphistudio_example_note_shift.dylib"
    entry = "plugin.dylib"
else:
    source_name = "libphistudio_example_note_shift.so"
    entry = "plugin.so"

source = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "target/release" / source_name
if source.suffix in {".so", ".dylib", ".dll"}:
    entry = "plugin" + source.suffix
if not source.is_file():
    raise SystemExit(f"Build the native cdylib first; missing {source}")
destination = root / "dist/native"
destination.mkdir(parents=True, exist_ok=True)
manifest = json.loads((root / "manifest.json").read_text())
manifest["id"] += "-native"
manifest["name"] = "Note shift native example"
manifest["description"] = "Move notes through the native C ABI and one Undo transaction."
manifest["backend"] = {"kind": "native", "entry": entry}
shutil.copy2(source, destination / entry)
(destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(destination)
