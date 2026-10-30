#!/usr/bin/env python3
"""Verify unchanged Phira HUD fonts and their APK attribution."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--apk", type=Path)
args = parser.parse_args()
source = ROOT / "assets/fonts/SOURCE.json"
manifest = json.loads(source.read_text())
for name, record in manifest["files"].items():
    data = (source.parent / name).read_bytes()
    assert len(data) == record["size"]
    assert hashlib.sha256(data).hexdigest() == record["sha256"], name
    if "git_blob" in record:
        assert hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest() == record["git_blob"]
if args.apk:
    with zipfile.ZipFile(args.apk) as apk:
        library = apk.read("lib/arm64-v8a/libphistudio_app.so")
        for name in manifest["files"]:
            assert (source.parent / name).read_bytes() in library, f"Font missing from APK: {name}"
        assert apk.read("assets/fonts/SOURCE.json") == source.read_bytes()
        for license_path in ["licenses/Phira-Ubuntu-Font-Licence.txt", "licenses/fastblur-LICENSE.txt"]:
            assert apk.read("assets/"+license_path) == (ROOT/license_path).read_bytes()
        assert apk.read("assets/THIRD_PARTY.md") == (ROOT/"THIRD_PARTY.md").read_bytes()
        assert b"Mivik" in apk.read("assets/fonts/SOURCE.json")
print("PASS: original Phira Ubuntu and PGR HUD fonts, Noto Emoji fallback; source hashes and attribution")
if args.apk: print("APK: font bytes and notices verified")
