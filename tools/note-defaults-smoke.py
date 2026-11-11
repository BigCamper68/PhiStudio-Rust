#!/usr/bin/env python3
"""Check new-note defaults, native gain, Save/history and preference persistence in two app sessions."""
import json
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

from x11_utils import capture_window, request_window_quit, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/note-defaults-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)
DEFAULTS = dict(size=1.5, speed=1.2, above=0, is_fake=1, alpha=180,
                y_offset=32., visible_time=8., judge_area=1.75,
                tint_hit_effects=[12, 34, 56], volume=0.6, sound=None)
PROPERTIES = dict(size="size", speed="speed", above="above", is_fake="isFake",
                  alpha="alpha", y_offset="yOffset", visible_time="visibleTime",
                  judge_area="judgeArea", tint_hit_effects="tintHitEffects")



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
            xdo("key", "--clearmodifiers", "1")  # Tap placement.
            click(285, 430)  # Empty note-grid lane in the private fixture.

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

            applied = saved(lambda raw: len(raw["judgeLineList"][0]["notes"]) == 1)
            expected = json.loads(json.dumps(before))
            note = applied["judgeLineList"][0]["notes"][0]
            expected["judgeLineList"][0]["notes"] = [note]
            expected["judgeLineList"][0]["numOfNotes"] = 1
            assert applied == expected, "Placement changed existing chart fields"
            assert note["type"] == 1
            for key, prop in PROPERTIES.items():
                assert note[prop] == DEFAULTS[key], (key, note)
            gain = note["phiStudioSoundGain"]
            assert gain["volume"] == 0.6 and gain["source"] is None
            assert gain["rendered"] == note["hitsound"]
            current = Path(json.loads(marker.read_text())["chart"])
            # Saved project metadata identifies its shared media directory.
            assets = Path(json.loads(marker.read_text())["assets"])
            assert (assets / gain["rendered"]).is_file(), "Baked hitsound was not saved"
            click(307, 20)
            saved(lambda raw: raw == before)
            click(344, 20)
            saved(lambda raw: raw == applied)
            capture_window(window, OUTPUT / f"redone-{number}.png")
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
    original = json.dumps(fixture).encode()
    source.write_bytes(original)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    preferences = storage / "editor-settings.json"
    preferences.write_text(json.dumps({"version": 1, "options": {
        "check_updates_on_start": False, "release_notes_seen": version,
        "new_note_defaults": DEFAULTS,
    }}))
    for number in [1, 2]:
        session(storage, source, number)
        assert json.loads(preferences.read_text())["options"]["new_note_defaults"] == DEFAULTS
        assert source.read_bytes() == original, "The external imported source changed"
    result = dict(pass_=True, sessions=2, all_note_properties=True, native_gain_saved=True, saved_preferences=True,
                  exact_undo_redo=True, imported_source_preserved=True)
    result["pass"] = result.pop("pass_")
    (OUTPUT / "result.json").write_text(json.dumps(result, indent=2))
    print("Note defaults main-loop smoke PASS:", result)
