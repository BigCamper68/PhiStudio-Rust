#!/usr/bin/env python3
"""Exercise event/BPM/line forms through real X11 input on a private demo copy.

Run under 1280x800 Xvfb after cargo build -p phistudio-app. Only this process,
its test window and artifacts/checks/authoring-input are touched.
"""
import json
from pathlib import Path
import shutil
import subprocess
import time
from x11_utils import capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "artifacts/checks/authoring-input"
OUT.mkdir(parents=True, exist_ok=True)
chart = OUT / "interactive.rpe.json"
shutil.copy2(ROOT / "assets/charts/phistudio-demo.rpe.json", chart)
original = json.loads(chart.read_text())


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.2)


def key(keys):
    xdo("key", "--clearmodifiers", keys)
    time.sleep(0.2)


def edit(x, y, value):
    click(x, y)
    key("ctrl+a")
    xdo("type", "--clearmodifiers", "--delay", "15", str(value))
    key("Return")


def wheel(x, y, down=True, steps=12):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "--repeat", steps, "--delay", "35", 5 if down else 4)
    time.sleep(0.25)


def save_expect(predicate, message):
    key("ctrl+s")
    deadline = time.monotonic() + 6
    while time.monotonic() < deadline:
        assert process.poll() is None, "Editor exited: " + message
        value = json.loads(chart.read_text())
        if predicate(value):
            return value
        time.sleep(0.05)
    raise AssertionError(message)


def moves(value):
    return value["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]


with (OUT / "editor.log").open("w") as log:
    process = subprocess.Popen([ROOT / "target/debug/phistudio", "--chart", chart], cwd=ROOT, stdout=log, stderr=log)
    window = None
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and process.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$"], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(0.05)
        assert window, "Editor window did not open"
        xdo("windowfocus", "--sync", window)
        wait_for_editor_paint(window, process)
        click(269, 756)  # + Event
        xdo("mousemove", "--sync", 570, 512)
        xdo("mousedown", "1")
        for y in range(512, 399, -8):
            xdo("mousemove", "--sync", 570, y)
            time.sleep(0.015)
        xdo("mousemove", "--sync", 570, 400)
        xdo("mouseup", "1")
        created = save_expect(lambda v: len(moves(v)) == 2, "Event creation did not save")
        assert moves(created)[1]["startTime"] == [2, 0, 1]
        assert moves(created)[1]["endTime"] == [3, 1, 4]
        capture_window(window, OUT / "events.png")
        edit(1056, 555, 135)
        wheel(1080, 600)
        capture_window(window, OUT / "event-scrolled.png")
        click(1000, 676)  # Apply event, at the bottom of the scrolled sidebar
        save_expect(lambda v: moves(v)[1]["end"] == 135, "Event values were not applied")
        key("ctrl+z")
        save_expect(lambda v: moves(v)[1]["end"] == 0, "Event value undo failed")
        key("ctrl+shift+z")
        save_expect(lambda v: moves(v)[1]["end"] == 135, "Event value redo failed")
        click(30, 756)  # Select
        click(570, 450)
        key("Delete")
        save_expect(lambda v: len(moves(v)) == 1, "Selected event deletion failed")
        key("ctrl+z")
        save_expect(lambda v: len(moves(v)) == 2, "Event deletion undo failed")

        click(1225, 282)  # Add a line
        save_expect(lambda v: len(v["judgeLineList"]) == 2, "Line creation failed")
        click(1010, 336)
        edit(245, 149, "Authoring smoke")
        edit(177, 305, 5)
        capture_window(window, OUT / "line-window.png")
        click(100, 497)
        save_expect(lambda v: v["judgeLineList"][1]["Name"] == "Authoring smoke" and v["judgeLineList"][1]["zOrder"] == 5, "Line properties did not apply")
        key("ctrl+z")
        save_expect(lambda v: v["judgeLineList"][1]["Name"] == "Line 1", "Line property undo failed")
        key("ctrl+shift+z")
        save_expect(lambda v: v["judgeLineList"][1]["Name"] == "Authoring smoke", "Line property redo failed")
        click(385, 82)  # Close line window

        click(1088, 336)
        click(140, 172)  # + BPM change
        capture_window(window, OUT / "bpm-window.png")
        edit(200, 220, "4")
        edit(140, 258, "180")
        click(140, 294)
        save_expect(lambda v: len(v["BPMList"]) == 2 and v["BPMList"][1]["startTime"] == [4, 0, 1] and v["BPMList"][1]["bpm"] == 180, "BPM change did not apply")
        key("ctrl+z")
        save_expect(lambda v: len(v["BPMList"]) == 1, "BPM undo failed")
        key("ctrl+shift+z")
        final = save_expect(lambda v: len(v["BPMList"]) == 2, "BPM redo failed")
        capture_window(window, OUT / "final.png")
        assert final["judgeLineList"][0]["notes"] == original["judgeLineList"][0]["notes"]
        (OUT / "result.txt").write_text("PASS\nReal X11: event create, value form, sidebar scrolling, delete and undo/redo\nLine create, name/zOrder form, undo/redo\nBPM add, beat/BPM form, undo/redo\nAll original notes remain unchanged\n")
        print("Authoring UI smoke PASS:", OUT)
    except BaseException:
        if window and process.poll() is None:
            capture_window(window, OUT / "failure.png")
        raise
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
