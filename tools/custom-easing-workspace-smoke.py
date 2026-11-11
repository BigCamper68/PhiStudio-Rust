#!/usr/bin/env python3
"""Check the curve workspace, draft ownership, Save and history in the main app."""
import json
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

from x11_utils import capture_window, request_window_quit, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/custom-easing-workspace-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.3)


def session(storage, source, number, expected=None):
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
            marker = storage / "last-project.json"
            deadline = time.monotonic() + 10
            while (not marker.exists() or marker.stat().st_mtime < started) and time.monotonic() < deadline:
                time.sleep(0.1)
            assert marker.exists() and marker.stat().st_mtime >= started

            def read():
                return json.loads(Path(json.loads(marker.read_text())["chart"]).read_text())

            def saved(predicate):
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    assert process.poll() is None, "Editor exited"
                    click(266, 20)
                    raw = read()
                    if predicate(raw):
                        return raw
                raise AssertionError("Saved chart did not reach the expected state")

            before = read()
            if expected is not None:
                assert before == expected, "Reopening changed the saved logical curve"
            click(565, 430)
            capture_window(window, OUTPUT / f"selected-{number}.png")
            if number == 1:
                click(1080, 244)
                xdo("key", "--clearmodifiers", "ctrl+a")
                xdo("type", "--clearmodifiers", "125")
                xdo("key", "Return")
            click(1040, 437)  # The actual event inspector opens the workspace.
            capture_window(window, OUTPUT / f"workspace-{number}.png")
            if number == 2:
                library = storage / "custom-easing/templates.json"
                templates = json.loads(library.read_text())["templates"]
                assert len(templates) == 1 and templates[0]["name"] == "Main loop"
                assert len(templates[0]["curve"]["nodes"]) == 3
                click(190, 778)  # Load the private template library in a fresh process.
                time.sleep(0.7)
                capture_window(window, OUTPUT / "library-reopened.png")
                xdo("key", "Escape")
                time.sleep(0.3)
                xdo("key", "Escape")
                time.sleep(0.3)
                saved(lambda raw: raw == expected)
                after = before
            else:
                click(105, 82)  # Add point.
                click(380, 300)  # Shape-preserving insertion on the graph.
                capture_window(window, OUTPUT / "canceled-draft.png")
                xdo("key", "Escape")
                time.sleep(0.3)
                saved(lambda raw: raw == before)
                click(1040, 437)
                click(105, 82)
                click(380, 300)
                capture_window(window, OUTPUT / "curve-draft.png")
                click(190, 778)  # Templates; the graph draft remains staged.
                time.sleep(0.7)
                click(80, 170)  # Save current curve under its existing name.
                library = storage / "custom-easing/templates.json"
                deadline = time.monotonic() + 10
                while not library.exists() and time.monotonic() < deadline:
                    time.sleep(0.1)
                templates = json.loads(library.read_text())["templates"]
                assert len(templates) == 1 and templates[0]["name"] == "Main loop"
                assert len(templates[0]["curve"]["nodes"]) == 3
                template_bytes = library.read_bytes()
                capture_window(window, OUTPUT / "library-saved.png")
                xdo("key", "Escape")
                time.sleep(0.3)
                click(50, 778)  # Use curve; worker completion returns an inspector draft.
                time.sleep(0.7)
                saved(lambda raw: raw == before)
                capture_window(window, OUTPUT / "inspector-draft.png")
                xdo("mousemove", "--sync", 1100, 665)
                xdo("click", "--repeat", "8", "--delay", "60", "5")
                time.sleep(0.3)
                click(1010, 676)
                capture_window(window, OUTPUT / "applied.png")

                def applied(raw):
                    event = raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]
                    return event["end"] == 125 and len(event["phiStudioCustomEasing"]["curve"]["nodes"]) == 3

                after = saved(applied)
                track = after["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
                assert len(track) == 1, "The workspace fragmented the authoring event"
                expected = json.loads(json.dumps(before))
                event = expected["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]
                event["end"] = 125
                event["phiStudioCustomEasing"] = track[0]["phiStudioCustomEasing"]
                assert after == expected, "The workspace changed unrelated source fields"
                xdo("mousemove", "--sync", 307, 20)
                click(307, 20)
                saved(lambda raw: raw == before)
                click(344, 20)
                saved(lambda raw: raw == after)
                assert library.read_bytes() == template_bytes, "Chart Undo/Redo changed the template library"
            path = Path(json.loads(marker.read_text())["chart"])
            request_window_quit(window)
            process.wait(timeout=15)
            assert process.returncode == 0
            return path, after
        except Exception:
            if window and process.poll() is None:
                capture_window(window, OUTPUT / f"failure-{number}.png")
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


with tempfile.TemporaryDirectory(prefix="phistudio-easing-workspace-") as private:
    storage = Path(private)
    source = storage / "source.rpe.json"
    fixture = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
    for line in fixture["judgeLineList"]:
        line["notes"] = []
    event = fixture["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].copy()
    event.update(startTime=[0, 0, 1], endTime=[8, 0, 1], start=10., end=100.,
                 phiStudioCustomEasing={"curve": {"version": 1, "name": "Main loop", "nodes": [
                     {"x": 0., "y": 0., "mode": "linear"}, {"x": 1., "y": 1., "mode": "linear"},
                 ]}, "domain": [0., 1.]}, unknownVendorData={"keep": [1, 2, 3]})
    fixture["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = [event]
    original = json.dumps(fixture).encode()
    source.write_bytes(original)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    (storage / "editor-settings.json").write_text(json.dumps({"version": 1, "options": {
        "check_updates_on_start": False, "release_notes_seen": version,
    }}))
    chart, after = session(storage, source, 1)
    session(storage, chart, 2, after)
    assert source.read_bytes() == original, "The external imported source changed"
    result = dict(main_loop=True, inspector_entry=True, graph_insertion=True, canceled_draft=True,
                  unsaved_properties=True, worker_draft=True, logical_event=True, exact_undo_redo=True,
                  reopened=True, source_preserved=True, template_saved=True, template_reopened=True,
                  library_outside_chart_history=True)
    result["pass"] = True
    (OUTPUT / "result.json").write_text(json.dumps(result, indent=2))
    print("Custom easing workspace main-loop smoke PASS:", result)
