#!/usr/bin/env python3
"""Verify accumulated project time across two actual application sessions."""
import json
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

from x11_utils import capture_window, request_window_quit, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/project-activity-runtime"
OUTPUT.mkdir(parents=True, exist_ok=True)


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def session(storage, index, before):
    with (OUTPUT / f"session-{index}.log").open("w") as log:
        process = subprocess.Popen([ROOT / "target/debug/phistudio", "--workspace", storage, "--size", "990x2145"],
                                   cwd=ROOT, stdout=log, stderr=log)
        window = None
        try:
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline and process.poll() is None:
                result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$"],
                                        capture_output=True, text=True)
                if result.returncode == 0:
                    window = result.stdout.splitlines()[-1]
                    break
                time.sleep(0.1)
            assert window, "Editor window did not open"
            xdo("windowfocus", "--sync", window)
            wait_for_editor_paint(window, process, height=2145, background=0x0c111d)
            time.sleep(1.5)  # Projects home must not accumulate editing time.
            record = storage / "activity" / "fixture.json"
            current = json.loads(record.read_text())["total_ms"] if record.exists() else 0
            assert current == before, "Projects home accumulated editing time"
            capture_window(window, OUTPUT / f"projects-{index}.png", width=990, height=2145)
            xdo("mousemove", "--sync", 495, 399)
            xdo("click", "1")
            time.sleep(1.5)
            capture_window(window, OUTPUT / f"editor-{index}.png", width=990, height=2145)
            request_window_quit(window)
            process.wait(timeout=15)
            assert process.returncode == 0, f"Editor exited with {process.returncode}"
        except Exception:
            if window and process.poll() is None:
                capture_window(window, OUTPUT / f"failure-{index}.png", width=990, height=2145)
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


with tempfile.TemporaryDirectory(prefix="phistudio-activity-") as private:
    storage = Path(private)
    project = storage / "projects" / "fixture"
    project.mkdir(parents=True)
    chart = project / "chart.json"
    raw = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
    chart.write_text(json.dumps(raw))
    original = chart.read_bytes()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    (storage / "editor-settings.json").write_text(json.dumps({
        "version": 1, "options": {"check_updates_on_start": False, "release_notes_seen": version},
    }))
    session(storage, 1, 0)
    record = storage / "activity" / "fixture.json"
    first = json.loads(record.read_text())["total_ms"]
    assert 300 <= first <= 5000, f"Projects home was counted or the editor was not counted: {first} ms"
    assert chart.read_bytes() == original, "Time checkpoint rewrote the chart"
    session(storage, 2, first)
    final = json.loads(record.read_text())["total_ms"]
    assert 300 <= final - first <= 5000, f"Second session did not accumulate correctly: {first} -> {final}"
    assert chart.read_bytes() == original, "Reopening or checkpointing rewrote the chart"
    result = {"first_ms": first, "final_ms": final, "two_sessions": True,
              "projects_home_excluded": True, "chart_bytes_preserved": True, "pass": True}
    (OUTPUT / "result.json").write_text(json.dumps(result, indent=2))
    print("Project activity main-loop smoke PASS:", result)
