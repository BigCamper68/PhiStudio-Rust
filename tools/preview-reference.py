#!/usr/bin/env python3
"""Compare rendered preview frames with the original, unchanged Phira GameScene.

Build the phistudio-prpr preview-reference example first, then run under Xvfb.
Generated inputs, stub icons for unused game screens, and PNGs stay in artifacts.
"""
import copy
import binascii
import json
from pathlib import Path
import shutil
import subprocess
import struct
import wave
import zlib

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/preview-reference"
OUTPUT.mkdir(parents=True, exist_ok=True)
root = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
root["META"].update(name="Preview reference", level="IN 12", background="cover.png", song="song.wav")
root["BPMList"] = [{"bpm": 120, "startTime": [0, 0, 1]}]
template = root["judgeLineList"][0]["notes"][0]
notes = []
for kind, start, end, fake, x in [(1,1,1,0,-300),(3,2,2,0,-100),(4,3,3,0,100),
                                  (2,4,6,0,300),(1,5,5,1,0),(1,7,7,0,-300)]:
    note = copy.deepcopy(template)
    note.update(type=kind, startTime=[start,0,1], endTime=[end,0,1], isFake=fake, positionX=x)
    notes.append(note)
root["judgeLineList"][0]["notes"] = notes
root["judgeLineList"][0]["numOfNotes"] = len(notes)
(OUTPUT / "chart.json").write_text(json.dumps(root))
attached = copy.deepcopy(root)
for i, element in enumerate(["pause", "combonumber", "combo", "score", "bar", "name", "level"]):
    line = copy.deepcopy(root["judgeLineList"][0])
    line.update(Name=f"HUD {element}", attachUI=element, notes=[], numOfNotes=0)
    layer = line["eventLayers"][0]
    for field, value in [("moveXEvents", 12+i*2), ("moveYEvents", 4+i), ("rotateEvents", 1+i*0.2), ("alphaEvents", 220+i*4)]:
        layer[field][0].update(start=value, end=value*1.5, endTime=[8,0,1])
    line["extended"].update(scaleXEvents=[dict(layer["moveXEvents"][0],start=0.9,end=1.1)],
                            scaleYEvents=[dict(layer["moveYEvents"][0],start=1.1,end=0.95)])
    attached["judgeLineList"].append(line)
(OUTPUT / "chart-attached.json").write_text(json.dumps(attached))
# Text events must use the unchanged Phira face: emoji are missing-glyph boxes,
# while CJK/Latin text and the original JSON strings remain intact.
text_chart = copy.deepcopy(root)
line = copy.deepcopy(root["judgeLineList"][0])
line.update(Name="Storyboard emoji reference", notes=[], numOfNotes=0)
line["eventLayers"][0]["moveYEvents"][0].update(start=180, end=180)
line["extended"]["textEvents"] = [dict(
    line["eventLayers"][0]["moveXEvents"][0], start="Save 😀 🎵 中文", end="Save 😀 🎵 中文", endTime=[16,0,1])]
text_chart["judgeLineList"].append(line)
(OUTPUT / "chart-text-emoji.json").write_text(json.dumps(text_chart, ensure_ascii=False))
with wave.open(str(OUTPUT / "song.wav"), "wb") as audio:
    audio.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
    audio.writeframes(b"\0\0" * 8000 * 8)
def png(width, height, pixel):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", binascii.crc32(kind + data))
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width): rows.extend(pixel(x, y))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


(OUTPUT / "cover.png").write_bytes(png(513,257,lambda x,y:
    ((x*19+y*3)%256, (y*29+x//5)%256, 240 if (x//32+y//32)%2 else 20)))
(OUTPUT / "wide-cover.png").write_bytes(png(2049,257,lambda x,y:
    ((x*13+y*3)%256,(y*23+x//9)%256,230 if (x//64+y//16)%2 else 15)))
for name in ["rank/F.png","rank/C.png","rank/B.png","rank/A.png","rank/S.png",
             "rank/V.png","rank/FC.png","rank/phi.png","mod/flip_x.png","mod/fade_out.png",
             "mod/fade_in.png","mod/nightcore.png","mod/rainbow.png","mod/autoplay.png",
             "mod/no-shader.png","back.png","retry.png","resume.png","proceed.png","player.jpg"]:
    path = OUTPUT / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png(2,2,lambda x,y: (255,255,255)))
pack = OUTPUT / "pack"
pack.mkdir(exist_ok=True)
for source in (ROOT / "assets/skins/phigros").iterdir():
    if source.name != "SOURCE.json": shutil.copyfile(source, pack / source.name)
shutil.copyfile(OUTPUT / "song.wav", pack / "ending.wav")
subprocess.run([ROOT / "target/debug/examples/preview-reference", OUTPUT], cwd=ROOT, check=True, timeout=90)
print((OUTPUT / "result.txt").read_text())
