#!/usr/bin/env python3
"""Exercise real mouse/keyboard input on a private copy of the Java demo.

Run under a 1280x800 Xvfb display after cargo build -p phistudio-app.
Only the process launched here and files in artifacts/checks/ui-input are touched.
"""
import json
from pathlib import Path
import shutil
import subprocess
import time
from x11_utils import wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/ui-input"
OUTPUT.mkdir(parents=True, exist_ok=True)
chart = OUTPUT / "interactive.rpe.json"
shutil.copy2(ROOT / "assets/charts/phistudio-demo.rpe.json", chart)


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def notes():
    return json.loads(chart.read_text())["judgeLineList"][0]["notes"]


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.2)


def key(keys):
    xdo("key", "--clearmodifiers", keys)
    time.sleep(0.2)


def save_expect(count):
    key("ctrl+s")
    return wait_for_notes(lambda rows: len(rows) == count, f"{count} saved notes")


def wait_for_notes(predicate, expected):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Editor exited during input test")
        current = notes()
        if predicate(current):
            return current
        time.sleep(0.05)
    raise AssertionError(f"Expected {expected}; saved notes: {notes()}")


with (OUTPUT / "editor.log").open("w") as log:
    process = subprocess.Popen([ROOT / "target/debug/phistudio", "--chart", chart], cwd=ROOT, stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 10
        window = None
        while time.monotonic() < deadline and process.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$"], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(0.1)
        assert window, "Editor window did not open"
        xdo("windowfocus", "--sync", window)
        wait_for_editor_paint(window, process)
        click(84, 756)  # Tap tool
        click(400, 512)  # Empty grid, away from the reference notes
        save_expect(5)
        key("ctrl+z")
        save_expect(4)
        key("ctrl+shift+z")
        save_expect(5)

        unchanged = notes()
        xdo("mousemove", "--sync", 400, 512)
        xdo("mousedown", "1")
        for y in range(512, 621, 9):
            xdo("mousemove", "--sync", 400, y)
            time.sleep(0.015)
        xdo("mouseup", "1")
        save_expect(5)
        assert notes() == unchanged, "Placement swipe edited notes instead of panning"
        click(1016, 20)  # Seek to the start using the chart overview

        click(129, 756)  # Hold tool
        xdo("mousemove", "--sync", 450, 600)
        xdo("mousedown", "1")
        for y in range(600, 399, -10):
            xdo("mousemove", "--sync", 450, y)
            time.sleep(0.015)
        xdo("mouseup", "1")
        time.sleep(0.2)
        save_expect(6)
        hold = notes()[-1]
        assert hold["type"] == 2 and hold["startTime"] != hold["endTime"], hold
        key("ctrl+z")  # A whole drag must be one history action
        save_expect(5)
        key("ctrl+shift+z")
        save_expect(6)
        original = notes()[4]
        click(31, 756)  # Select tool
        click(421, 512)  # First select; a subsequent gesture moves the note
        xdo("mousemove", "--sync", 421, 512)
        xdo("mousedown", "1")
        for step in range(1, 11):
            xdo("mousemove", "--sync", 421 - 8 * step, 512 - 4 * step)
            time.sleep(0.02)
        xdo("mouseup", "1")
        time.sleep(0.2)
        save_expect(6)
        moved = wait_for_notes(lambda rows: rows[4]["positionX"] != original["positionX"], "moved Tap")[4]
        key("ctrl+z")
        save_expect(6)
        wait_for_notes(lambda rows: rows[4] == original, "undo restores the original Tap")
        key("ctrl+shift+z")
        save_expect(6)
        wait_for_notes(lambda rows: rows[4] == moved, "redo restores the moved Tap")
        (OUTPUT / "result.txt").write_text("PASS\nPointer: create Tap\nKeyboard: undo/redo/save\nPlacement swipe pans without changing notes\nPointer: create Hold by dragging\nSingle undo removes the complete Hold gesture\nPointer: move Tap; undo/redo restore exact note JSON\nSaved chart: 6 notes\n")
        print("UI input smoke test PASS:", OUTPUT)
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
