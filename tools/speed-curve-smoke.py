#!/usr/bin/env python3
"""Exercise graph editing through the actual application's main loop on Xvfb.

Run on a 1280x800 display after building phistudio-app. Only a private chart and
the process started here are modified; no alternate UI entry point is used.
"""
import json
from pathlib import Path
import subprocess
import time
from x11_utils import capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/speed-curve-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)
CHART = OUTPUT / "interactive.rpe.json"
fixture = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
fixture["BPMList"] = [{"bpm": 120, "startTime": [0, 0, 1]}]
fixture["judgeLineList"][0]["eventLayers"][0]["speedEvents"] = [{
    "startTime": [0, 0, 1], "endTime": [8, 0, 1], "start": 10, "end": 10,
    "easingType": 1, "custom": "runtime smoke",
}]
CHART.write_text(json.dumps(fixture))


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.2)


def key(keys):
    xdo("key", "--clearmodifiers", keys)
    time.sleep(0.15)


def saved(predicate, message):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        assert process.poll() is None, "Editor exited"
        key("ctrl+s")
        raw = json.loads(CHART.read_text())
        if predicate(raw):
            return raw
        time.sleep(0.1)
    raise AssertionError(message)


with (OUTPUT / "editor.log").open("w") as log:
    process = subprocess.Popen([ROOT / "target/debug/phistudio", "--chart", CHART],
                               cwd=ROOT, stdout=log, stderr=log)
    window = None
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline and process.poll() is None:
            result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$"],
                                    capture_output=True, text=True)
            if result.returncode == 0:
                window = result.stdout.splitlines()[-1]
                break
            time.sleep(0.1)
        assert window, "Editor window did not open"
        xdo("windowfocus", "--sync", window)
        wait_for_editor_paint(window, process)
        click(84, 756)  # Tap tool.
        click(400, 512)  # Create a note away from the demo notes.
        before = saved(lambda raw: len(raw["judgeLineList"][0]["notes"]) == 5,
                       "The private fixture note was not created")
        (OUTPUT / "before.json").write_text(json.dumps(before, indent=2))
        click(31, 756)  # Select tool.
        click(421, 512)
        capture_window(window, OUTPUT / "properties.png")
        click(1060, 640)  # Note speed curve in the actual note inspector.
        capture_window(window, OUTPUT / "opened.png")
        click(150, 138)  # Add point.
        xdo("mousemove", "--sync", 263, 435)
        xdo("mousedown", "1")
        time.sleep(0.3)  # The runtime must retain capture over multiple frames.
        xdo("mouseup", "1")
        added = saved(lambda raw: "phiStudioSpeedCurves" in raw["judgeLineList"][0],
                      "Main-loop input cancellation prevented adding a point")
        assert added["judgeLineList"][0]["eventLayers"][0]["speedEvents"] != before["judgeLineList"][0]["eventLayers"][0]["speedEvents"]
        assert added["judgeLineList"][0]["notes"] == before["judgeLineList"][0]["notes"]
        (OUTPUT / "added.json").write_text(json.dumps(added, indent=2))
        capture_window(window, OUTPUT / "added.png")
        click(60, 138)  # Edit point.
        xdo("mousemove", "--sync", 263, 435)
        xdo("mousedown", "1")
        for y in range(435, 465, 3):
            xdo("mousemove", "--sync", 263, y)
            time.sleep(0.04)
        xdo("mouseup", "1")
        edited = saved(lambda raw: raw != added, "Main loop canceled the point drag")
        (OUTPUT / "dragged.json").write_text(json.dumps(edited, indent=2))
        capture_window(window, OUTPUT / "dragged.png")
        click(292, 20)  # Actual main-window Undo, reachable after selection clears.
        saved(lambda raw: raw == added, "One Undo did not restore the added point")
        click(292, 20)  # Actual main-window Undo, reachable after selection clears.
        saved(lambda raw: raw == before, "Second Undo did not restore the complete original chart")
        click(331, 20)  # Actual main-window Redo.
        saved(lambda raw: raw == added, "Redo did not restore the point")
        click(331, 20)  # Actual main-window Redo.
        saved(lambda raw: raw == edited, "Redo did not restore the drag")
        (OUTPUT / "result.json").write_text(json.dumps({
            "main_loop": True, "point_add": True, "point_drag": True,
            "exact_undo_redo": True, "pass": True,
        }, indent=2))
        print("Speed curve main-loop smoke PASS:", OUTPUT)
    except Exception:
        if window and process.poll() is None:
            capture_window(window, OUTPUT / "failure.png")
        raise
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
