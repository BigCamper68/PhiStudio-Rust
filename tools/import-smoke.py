#!/usr/bin/env python3
"""Run package/resource integration checks with real prpr GL rendering under Xvfb."""
import binascii
import io
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import zipfile
import zlib
import wave

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/import-render"
OUTPUT.mkdir(parents=True, exist_ok=True)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", binascii.crc32(kind + data))


def solid_png():
    header = struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0)
    rows = (b"\0" + bytes([200, 50, 100, 255]) * 2) * 2
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def pixels(path):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    position, compressed = 8, bytearray()
    while position < len(data):
        length = struct.unpack(">I", data[position:position + 4])[0]
        kind = data[position + 4:position + 8]
        body = data[position + 8:position + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, color, interlace) == (8, 6, 0)
        if kind == b"IDAT":
            compressed.extend(body)
        position += 12 + length
    raw, stride = zlib.decompress(compressed), width * 4
    previous, rows = bytearray(stride), []
    def paeth(a, b, c):
        estimate = a + b - c
        distances = [abs(estimate - value) for value in (a, b, c)]
        return (a, b, c)[distances.index(min(distances))]
    for y in range(height):
        offset = y * (stride + 1)
        mode, row = raw[offset], bytearray(raw[offset + 1:offset + 1 + stride])
        assert 0 <= mode <= 4
        for i in range(stride):
            left, above, corner = row[i - 4] if i >= 4 else 0, previous[i], previous[i - 4] if i >= 4 else 0
            predictor = (0, left, above, (left + above) // 2, paeth(left, above, corner))[mode]
            row[i] = (row[i] + predictor) & 255
        rows.append(row)
        previous = row
    return lambda x, y: tuple(rows[y][x * 4:x * 4 + 4])


def near(actual, expected):
    assert all(abs(a - b) <= 1 for a, b in zip(actual, expected)), (actual, expected)


demo = (ROOT / "assets/charts/phistudio-demo.rpe.json").read_bytes()
music = io.BytesIO()
with wave.open(music, "wb") as audio:
    audio.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
    audio.writeframes(b"\0\0" * 8000)
pec = b"150\nbp 0 120\ncp 0 0 1024 700\ncd 0 0 0\nca 0 0 255\ncv 0 0 5.85\nn1 0 1 0 1 0\nn2 0 2 4 512 1 0\nn3 0 3 -512 1 0\nn4 0 4 0 1 0\n"
official = (ROOT / "crates/phistudio-core/tests/fixtures/conversion/official-v3.json").read_bytes()
for name, background, source, expected_format in [
    ("illustration", solid_png(), demo, "rpe"),
    ("broken-illustration", b"invalid image", demo, "rpe"),
    ("pec", solid_png(), pec, "legacy-pec"),
    ("official", solid_png(), official, "official-phigros"),
]:
    directory = OUTPUT / name
    directory.mkdir(exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix="workspace-", dir=directory))
    archive = directory / "probe.pez"
    manifest = b"name: Resource probe\nchart: demo.data\nillustration: cover.png\nmusic: song.wav\nbackgroundDim: 0.4\naspectRatio: 1.5\noffset: -0.0015\nnoteUniformScale: true\nuseRpe170Speed: true\n"
    with zipfile.ZipFile(archive, "w") as package:
        package.writestr("demo.data", source)
        package.writestr("cover.png", background)
        package.writestr("song.wav", music.getvalue())
        package.writestr("info.yml", manifest)
    with (directory / "editor.log").open("w") as log:
        subprocess.run([ROOT / "target/debug/phistudio", "--workspace", workspace, "--package", archive,
                        "--smoke-test", directory], cwd=ROOT, stdout=log, stderr=log, check=True, timeout=45)
    options = json.loads((directory / "project-options.json").read_text())
    assert options["music"] == "song.wav" and options["illustration"] == "cover.png"
    assert options["manifest_offset_ms"] == -2
    assert options["aspect_ratio"] == 1.5 and options["note_uniform_scale"] and options["use_rpe_170_speed"]
    edited = json.loads((directory / "edited.rpe.json").read_text())
    if expected_format == "rpe":
        assert edited["META"] == json.loads(demo)["META"]
    else:
        assert edited["META"]["name"] == "Resource probe"
    location = json.loads((workspace / "last-project.json").read_text())
    assets = Path(location["assets"])
    assert (assets / "demo.data").read_bytes() == source
    assert json.loads(Path(location["chart"]).read_text())["META"] == edited["META"]
    description = next(workspace.glob("projects/*/.phistudio-workspace.json"))
    assert json.loads(description.read_text())["charts"][0]["format"] == expected_format
    assert (assets / "cover.png").read_bytes() == background
    assert (assets / "info.yml").read_bytes() == manifest
    image = pixels(directory / "prpr-preview.png")
    warning = (directory / "resource-warning.txt").read_text()
    if name != "broken-illustration":
        assert not warning, warning
        near(image(50, 50), (140, 35, 70, 255))  # Phira's full-frame 30% dim.
        near(image(200, 50), (84, 21, 42, 255))  # Additional 40% chart-area dim.
    else:
        assert "Could not load illustration" in warning
        near(image(50, 50), (10, 15, 27, 255))
        near(image(200, 50), (10, 15, 27, 255))
    cyan = 0
    for y in range(720):
        for x in range(1280):
            red, green, blue, alpha = image(x, y)
            if blue > 160 and green > 130 and blue > red + 20:
                cyan += 1
    assert cyan > 10, f"No rendered Tap/Hold texture pixels: {name}"
    print("PASS:", name, expected_format, "note pixels:", cyan, flush=True)
(OUTPUT / "result.txt").write_text("PASS\nRPE/PEC/official Phigros content import and persisted ZIP/PEZ package options\nOriginal chart, manifest and resources remain byte-identical\nManifest display does not overwrite authored RPE META\nOriginal prpr renders the illustration with dim and aspect ratio\nInvalid illustration produces a warning; note rendering, editing and save still work\n")
