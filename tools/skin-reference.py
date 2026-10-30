#!/usr/bin/env python3
"""Generate an unmistakable Phira resource skin and compare original GameScene.

Uses only generated fixtures and copies of the embedded default pack in artifacts.
Run preview-reference.py first and build the preview-reference Rust example.
"""
from pathlib import Path
import array, binascii, json, math, shutil, struct, subprocess, wave, zipfile, zlib

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'artifacts/checks/preview-reference'
OUT = ROOT / 'artifacts/checks/skin-reference'
OUT.mkdir(parents=True, exist_ok=True)
for item in BASE.iterdir():
    if item.name in ['pack', 'rank', 'mod']:
        shutil.copytree(item, OUT / item.name, dirs_exist_ok=True)
    elif item.name in ['chart.json', 'chart-attached.json', 'cover.png', 'wide-cover.png',
                       'song.wav', 'back.png', 'retry.png', 'resume.png', 'proceed.png', 'player.jpg']:
        shutil.copy2(item, OUT / item.name)
pack = OUT / 'pack'
pack.joinpath('info.yml').write_text('''name: Preview Skin Fixture
author: PhiStudio Tests
description: Generated textures, highlighted notes, repeated Hold body and independent audio cues
hitFx: [5, 6]
hitFxDuration: 0.65
hitFxScale: 0.9
hitFxRotate: true
hitFxTinted: false
hideParticles: true
holdAtlas: [50, 50]
holdAtlasMH: [50, 95]
holdRepeat: true
holdKeepHead: true
colorPerfect: 0x33CC77
colorGood: 0xB245EE
''')

def png(width, height, highlighted):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', binascii.crc32(kind + data))
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            c = (35, 240, 85) if highlighted else (235, 65, 30)
            if (x // 16 + y // 16) % 2:
                c = tuple(v // 2 for v in c)
            rows.extend((*c, 255))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))

for path in pack.glob('*.png'):
    width, height = struct.unpack('>II', path.read_bytes()[16:24])
    path.write_bytes(png(width, height, '_mh' in path.name))
for name, frequency in [('click', 730), ('flick', 1170), ('drag', 510)]:
    pack.joinpath(name + '.ogg').unlink(missing_ok=True)
    with wave.open(str(pack / (name + '.wav')), 'wb') as out:
        out.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
        values = array.array('h', (round(8500 * math.sin(2 * math.pi * frequency * i / 48000)
                                        * (1 - i / 7200)) for i in range(7200)))
        out.writeframes(values.tobytes())
for name in ['chart.json', 'chart-attached.json']:
    path = OUT / name
    chart = json.loads(path.read_text())
    note = dict(chart['judgeLineList'][0]['notes'][0])
    note.update(type=1, startTime=[4, 0, 1], endTime=[4, 0, 1], positionX=-100)
    chart['judgeLineList'][0]['notes'].append(note)
    chart['judgeLineList'][0]['numOfNotes'] += 1
    path.write_text(json.dumps(chart))
OUT.joinpath('use-custom-skin').touch()
archive = OUT / 'Preview Skin Fixture.zip'
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as out:
    for path in pack.iterdir():
        out.write(path, 'Fixture/' + path.name)
subprocess.run([ROOT / 'target/debug/examples/preview-reference', OUT], cwd=ROOT, check=True, timeout=120)
report = json.loads(OUT.joinpath('report.json').read_text())
assert len(report) == 28 and all(row['different_rgb_pixels'] == 0 for row in report)
assert all(row['skin']['name'] == 'Preview Skin Fixture' and row['skin']['hold_repeat'] for row in report)
print('PASS: custom normal/highlighted textures, repeating Holds and line colors match original GameScene')
