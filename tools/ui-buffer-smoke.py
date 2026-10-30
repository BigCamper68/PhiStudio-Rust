#!/usr/bin/env python3
"""Exercise real egui GPU buffer growth with a dense synthetic event timeline.

Run under Xvfb after building phistudio-app. Fixtures, frames and saved charts
are confined to artifacts/checks/ui-buffers. No user chart data is required.
"""
import copy
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "artifacts/checks/ui-buffers"
OUT.mkdir(parents=True, exist_ok=True)
chart = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
chart["META"]["name"] = "Dense event mesh"
template = chart["judgeLineList"][0]["eventLayers"][0]["alphaEvents"][0]
events = []
for tick in range(320):
    event = copy.deepcopy(template)
    event.update(startTime=[tick // 32, tick % 32, 32],
                 endTime=[(tick + 1) // 32, (tick + 1) % 32, 32],
                 start=255 if tick % 2 else 0, end=0 if tick % 2 else 255)
    events.append(event)
chart["judgeLineList"][0]["eventLayers"][0]["alphaEvents"] = events
fixture = OUT / "dense.rpe.json"
fixture.write_text(json.dumps(chart))
for size in ["1280x800", "873x393"]:
    output = OUT / size
    output.mkdir(exist_ok=True)
    with (output / "editor.log").open("w") as log:
        subprocess.run([ROOT / "target/debug/phistudio", "--chart", fixture,
                        "--size", size, "--smoke-test", output], cwd=ROOT,
                       stdout=log, stderr=log, check=True, timeout=75)
    assert (output / "smoke-test.txt").read_text().startswith("PASS")
    saved = json.loads((output / "edited.rpe.json").read_text())
    assert saved["judgeLineList"][0]["eventLayers"][0]["alphaEvents"] == events
    print("Dense event GPU buffers PASS:", size, flush=True)
(OUT / "result.txt").write_text("PASS\n320 synthetic alpha intervals; full/compact real GL render, edit, undo/redo and save\n")
