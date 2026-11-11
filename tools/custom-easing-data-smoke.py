#!/usr/bin/env python3
"""Check logical custom easing, property editing and Save/history in the actual app."""
import json
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

from x11_utils import capture_window, request_window_quit, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/custom-easing-data-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)
BINDING = {"curve": {"version": 1, "name": "Motion", "nodes": [
    {"x": 0.0, "y": 0.0, "mode": "corner", "out": [0.1, 1.5]},
    {"x": 0.55, "y": 0.55, "mode": "corner", "in": [0.4, -0.25], "out": [0.7, 1.4]},
    {"x": 1.0, "y": 1.0, "mode": "corner", "in": [0.9, 0.6]},
]}, "domain": [0.0, 1.0]}



def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.25)


def session(storage, source, number):
    with (OUTPUT / f"session-{number}.log").open("w") as log:
        started = time.time()
        process = subprocess.Popen([
            ROOT / "target/debug/phistudio", "--workspace", storage, "--chart", source,
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
            deadline = time.monotonic() + 10
            marker = storage / "last-project.json"
            while (not marker.exists() or marker.stat().st_mtime < started) and time.monotonic() < deadline:
                time.sleep(0.1)
            assert marker.exists() and marker.stat().st_mtime >= started, "Active project location was not recorded"
            # Managed imports edit their copied chart, not the external input.
            chart = Path(json.loads(marker.read_text())["chart"])
            before = json.loads(chart.read_text())
            capture_window(window, OUTPUT / f"before-{number}.png")
            click(565, 430)  # Select the single Move X event.
            capture_window(window, OUTPUT / f"selected-{number}.png")
            click(1080, 234)  # End value in the actual inspector.
            xdo("key", "--clearmodifiers", "ctrl+a")
            xdo("type", "--clearmodifiers", "125")
            xdo("key", "Return")
            time.sleep(0.2)
            capture_window(window, OUTPUT / f"draft-{number}.png")
            xdo("mousemove", "--sync", 1100, 665)
            xdo("click", "--repeat", "8", "--delay", "60", "5")
            time.sleep(0.4)
            capture_window(window, OUTPUT / f"before-apply-{number}.png")
            click(1010, 676)  # Apply event after scrolling the inspector.
            capture_window(window, OUTPUT / f"applied-{number}.png")

            def saved(predicate):
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    assert process.poll() is None, "Editor exited"
                    click(266, 20)
                    # Save may publish a new working generation/location.
                    current = Path(json.loads(marker.read_text())["chart"])
                    raw = json.loads(current.read_text())
                    if predicate(raw):
                        return raw
                raise AssertionError("Saved chart did not reach the expected state")

            applied = saved(lambda raw: raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["end"] == 125)
            expected = json.loads(json.dumps(before))
            expected["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["end"] = 125
            assert applied == expected, "Property editing changed curve data or unrelated fields"
            track = applied["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
            assert len(track) == 1 and track[0]["phiStudioCustomEasing"] == BINDING
            click(307, 20)
            saved(lambda raw: raw == before)
            click(344, 20)
            saved(lambda raw: raw == applied)
            capture_window(window, OUTPUT / f"redone-{number}.png")
            click(565, 430)  # History navigation may clear selection.
            capture_window(window, OUTPUT / f"before-standard-{number}.png")
            click(1020, 358)  # Use standard easing; only the inspector draft changes.
            xdo("mousemove", "--sync", 1100, 665)
            xdo("click", "--repeat", "8", "--delay", "60", "5")
            time.sleep(0.3)
            capture_window(window, OUTPUT / f"before-standard-apply-{number}.png")
            click(1010, 676)  # Apply the explicit return to standard interpolation.
            cleared = saved(lambda raw: "phiStudioCustomEasing" not in raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0])
            expected = json.loads(json.dumps(applied))
            del expected["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["phiStudioCustomEasing"]
            assert cleared == expected, "Switching to standard easing changed unrelated fields"
            click(307, 20)
            saved(lambda raw: raw == applied)
            click(344, 20)
            saved(lambda raw: raw == cleared)
            click(307, 20)
            saved(lambda raw: raw == applied)
            request_window_quit(window)
            process.wait(timeout=15)
            assert process.returncode == 0
        except Exception:
            if window and process.poll() is None:
                capture_window(window, OUTPUT / f"failure-{number}.png")
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


with tempfile.TemporaryDirectory(prefix="phistudio-defaults-") as private:
    storage = Path(private)
    source = storage / "source.rpe.json"
    fixture = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
    for line in fixture["judgeLineList"]:
        line["notes"] = []
    fixture["privateUnknownExtension"] = {"preserve": [1, 2, 3]}
    event = fixture["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].copy()
    event.update(startTime=[0, 0, 1], endTime=[8, 0, 1], start=10., end=100.,
                 easingType=1, bezier=0, phiStudioCustomEasing=BINDING,
                 unknownVendorData={"keep": [1, 2, 3]})
    fixture["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = [event]
    original = json.dumps(fixture).encode()
    source.write_bytes(original)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    preferences = storage / "editor-settings.json"
    preferences.write_text(json.dumps({"version": 1, "options": {
        "check_updates_on_start": False, "release_notes_seen": version,

    }}))
    for number in [1]:
        session(storage, source, number)
        assert source.read_bytes() == original, "The external imported source changed"
    result = dict(pass_=True, main_loop=True, logical_event=True, curve_preserved=True,
                  property_edit=True, standard_restore=True, exact_undo_redo=True, imported_source_preserved=True)
    result["pass"] = result.pop("pass_")
    (OUTPUT / "result.json").write_text(json.dumps(result, indent=2))
    print("Custom easing data main-loop smoke PASS:", result)
