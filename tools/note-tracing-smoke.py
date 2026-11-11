#!/usr/bin/env python3
"""Exercise Motion Generator tracing, Save and history in the actual main loop.

Run on a private 1280x800 X11 display after building phistudio-app. All chart
and UI changes belong to the process and fixture started here.
"""
import json
from pathlib import Path
import subprocess
import time

from x11_utils import capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/note-tracing-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)
CHART = OUTPUT / "interactive.rpe.json"
(OUTPUT / "result.json").unlink(missing_ok=True)


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.25)


def key(keys):
    xdo("key", "--clearmodifiers", keys)
    time.sleep(0.2)


def wheel(count):
    xdo("click", "--repeat", count, "--delay", 80, "5")
    time.sleep(0.4)


def saved(predicate, message):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        assert process.poll() is None, "Editor exited"
        click(266, 20)  # Save remains reachable when a tool button owns focus.
        raw = json.loads(CHART.read_text())
        if predicate(raw):
            return raw
        time.sleep(0.1)
    raise AssertionError(message)


fixture = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
for line in fixture["judgeLineList"][1:]:
    line["notes"] = []
note = fixture["judgeLineList"][0]["notes"][0].copy()
note.update(startTime=[4, 0, 1], endTime=[4, 0, 1], positionX=0)
fixture["judgeLineList"][0]["notes"] = [note]
CHART.write_text(json.dumps(fixture))
with (OUTPUT / "editor.log").open("w") as log:
    process = subprocess.Popen([
        ROOT / "target/debug/phistudio", "--chart", CHART,
    ], cwd=ROOT, stdout=log, stderr=log)
    window = None
    try:
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline and process.poll() is None:
            result = subprocess.run([
                "xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$",
            ], capture_output=True, text=True)
            if result.returncode == 0:
                window = result.stdout.splitlines()[-1]
                break
            time.sleep(0.1)
        assert window, "Editor window did not open"
        xdo("windowfocus", "--sync", window)
        wait_for_editor_paint(window, process)
        key("ctrl+a")  # The private fixture has exactly one note.
        click(516, 756)  # Tools.
        click(230, 125)  # Tool group.
        xdo("mousemove", "--sync", 225, 280)
        wheel(3)
        click(210, 228)  # Motion generator in the scrolled group menu.
        click(182, 173)  # Note Tracing.
        capture_window(window, OUTPUT / "options.png")
        click(110, 563)  # Preview tracing.
        time.sleep(2)
        saved(lambda raw: raw == fixture, "Preview changed the source chart")
        capture_window(window, OUTPUT / "preview.png")
        xdo("mousemove", "--sync", 260, 590)
        wheel(8)
        capture_window(window, OUTPUT / "before-apply.png")
        click(110, 644)  # Apply tracing, at the bottom of Tools content.
        capture_window(window, OUTPUT / "after-apply.png")
        applied = saved(
            lambda raw: len(raw["judgeLineList"]) == len(fixture["judgeLineList"]) + 1,
            "Main-loop tracing did not create a destination line",
        )
        assert applied["judgeLineList"][:-1] == fixture["judgeLineList"], "Source line changed"
        (OUTPUT / "applied.json").write_text(json.dumps(applied, indent=2))
        click(307, 20)  # Actual main-window Undo.
        saved(lambda raw: raw == fixture, "One Undo did not restore the original chart")
        click(344, 20)  # Actual main-window Redo.
        saved(lambda raw: raw == applied, "Redo did not restore the generated line")
        capture_window(window, OUTPUT / "redone.png")
        result = {"main_loop": True, "preview_does_not_edit": True, "new_line": True,
                  "source_preserved": True, "save": True, "exact_undo_redo": True, "pass": True}
        (OUTPUT / "result.json").write_text(json.dumps(result, indent=2))
        print("Note Tracing main-loop smoke PASS:", OUTPUT)
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
