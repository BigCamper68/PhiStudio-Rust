#!/usr/bin/env python3
"""Verify pinned Noise Fields assets, optionally re-extracting from original Unity data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/phistudio-prpr/src/noise_fields"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--original-assets", type=Path, help="Assembled sharedassets12.assets; requires UnityPy 1.25.4")
parser.add_argument("--xapk", type=Path, help="Check the supplied original XAPK checksum")
args = parser.parse_args()
manifest = json.loads((SOURCE / "SOURCE.json").read_text())
for name, record in manifest["files"].items():
    path = SOURCE / name
    assert path.stat().st_size == record["size"], name
    assert sha256(path) == record["sha256"], name
if args.xapk:
    assert sha256(args.xapk) == manifest["xapk_sha256"], "Different source XAPK"
if args.original_assets:
    key = "assets/bin/Data/sharedassets12.assets (assembled splits)"
    assert sha256(args.original_assets) == manifest["original_files"][key]["sha256"]
    import UnityPy
    settings = {}
    for obj in UnityPy.load(str(args.original_assets)).objects:
        if obj.type.name != "Texture2D":
            continue
        data = obj.read_typetree()
        if data["m_Name"] in manifest["texture_settings"]:
            native = data["m_TextureSettings"]
            assert native["m_WrapU"] == native["m_WrapV"] == native["m_WrapW"]
            settings[data["m_Name"]] = {
                "filter": ["Point", "Bilinear", "Trilinear"][native["m_FilterMode"]],
                "wrap": ["Repeat", "Clamp", "Mirror"][native["m_WrapU"]],
            }
    assert settings == manifest["texture_settings"], "Texture sampler settings differ from Unity source"
    with tempfile.TemporaryDirectory(prefix="phistudio-noise-verify-") as directory:
        output = Path(directory)
        subprocess.run([sys.executable, str(ROOT / "tools/extract-noise-fields.py"), str(args.original_assets), "--output", str(output)], check=True)
        generated = {str(p.relative_to(output)) for p in output.rglob("*") if p.is_file()}
        assert generated == set(manifest["files"]), "Unexpected re-extracted file set"
        for name in generated:
            assert (output / name).read_bytes() == (SOURCE / name).read_bytes(), name
print(f"PASS: {len(manifest['files'])} Noise Fields shader/material/texture hashes and source attribution")
if args.original_assets:
    print("PASS: all bundled Noise Fields assets reproduced byte-for-byte from the supplied Unity data")
