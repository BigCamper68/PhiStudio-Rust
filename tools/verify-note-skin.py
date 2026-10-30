#!/usr/bin/env python3
"""Verify the embedded skin against its provenance, supplied ZIP or built APK."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1] / "assets/skins/phigros"
manifest = json.loads((ROOT / "SOURCE.json").read_text())
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--archive", type=Path)
parser.add_argument("--apk", type=Path)
args = parser.parse_args()

for name, record in manifest["files"].items():
    data = (ROOT / name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == record["sha256"], name
    assert len(data) == record["bytes"], name

if args.archive:
    with args.archive.open("rb") as source:
        assert hashlib.file_digest(source, "sha256").hexdigest() == manifest["archive_sha256"]
    with ZipFile(args.archive) as archive:
        assert set(archive.namelist()) == set(manifest["files"])
        for name in archive.namelist():
            assert archive.read(name) == (ROOT / name).read_bytes(), name

if args.apk:
    with ZipFile(args.apk) as apk:
        native = apk.read("lib/arm64-v8a/libphistudio_app.so")
        for name in manifest["files"]:
            assert (ROOT / name).read_bytes() in native, f"Skin asset missing from the APK: {name}"
        assert json.loads(apk.read("assets/skin/SOURCE.json")) == manifest
        assert apk.read("assets/skin/info.yml") == (ROOT / "info.yml").read_bytes()

for name, size in [("click.png", (989, 100)), ("hold.png", (989, 2000)), ("hold_mh.png", (1056, 1997))]:
    assert struct.unpack(">II", (ROOT / name).read_bytes()[16:24]) == size, name
print(f'PASS: {len(manifest["files"])} unchanged {manifest["name"]} assets by {manifest["author"]}')
if args.apk:
    print("APK: original pack bytes and attribution verified")
