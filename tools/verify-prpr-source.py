#!/usr/bin/env python3
"""Verify the vendored prpr files against the pinned upstream Git blobs."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "vendor/phira"
manifest = json.loads((root / "UPSTREAM.json").read_text())
assert manifest["revision"] == "0afd16621dd66557841a8cffe6813afff63335ec"
for name, expected in manifest["files"].items():
    data = (root / name).read_bytes()
    blob = b"blob " + str(len(data)).encode() + b"\0" + data
    assert hashlib.sha256(data).hexdigest() == expected["sha256"], name
    assert hashlib.sha1(blob).hexdigest() == expected["git_blob"], name
original = (root / "UPSTREAM-Cargo.toml").read_text()
a = original.index("members = [")
b = original.index("]", a) + 1
expected_workspace = original[:a] + 'members = ["prpr", "prpr-auto-offset", "prpr-l10n", "prpr-avc"]' + original[b:]
assert (root / "Cargo.toml").read_text() == expected_workspace
print(f'PASS: {len(manifest["files"])} unchanged upstream files; prpr {manifest["revision"]}')
