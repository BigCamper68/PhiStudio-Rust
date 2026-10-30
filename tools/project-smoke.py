#!/usr/bin/env python3
"""Exercise library and process-death recovery through real X11 input.

Run under Xvfb at 1280x800 after building phistudio-app. Only this script's
process and a new workspace under artifacts/checks/project-input are modified.
"""
import ctypes as C
import ctypes.util
import json
import math
import wave
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import zlib

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/checks/project-input-0.9"
OUTPUT.mkdir(parents=True, exist_ok=True)
WORKSPACE = Path(tempfile.mkdtemp(prefix="workspace-", dir=OUTPUT))
process = None
window = None
logs = []


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def wait_for(predicate, message, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        assert process is not None and process.poll() is None, "Editor exited: " + message
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    capture("failure.png")
    raise AssertionError(message)


def click(x, y):
    xdo("mousemove", "--sync", x, y)
    xdo("click", "1")
    time.sleep(0.25)


def key(keys):
    xdo("key", "--clearmodifiers", keys)
    time.sleep(0.25)


def start(number):
    global process, window
    log = (OUTPUT / f"editor-{number}.log").open("w")
    logs.append(log)
    process = subprocess.Popen([ROOT / "target/debug/phistudio", "--workspace", WORKSPACE], cwd=ROOT, stdout=log, stderr=log)
    def find_window():
        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^PhiStudio Rust$"], capture_output=True, text=True)
        return found.stdout.splitlines()[-1] if found.returncode == 0 else None
    window = wait_for(find_window, "Editor window did not open")
    xdo("windowfocus", "--sync", window)
    # Resource-pack decoding occurs after the native window is mapped.
    wait_for(lambda: capture("startup.png"), "Editor did not paint its first frame", seconds=20)
    time.sleep(0.25)


def stop():
    # No save/quit callback: this must exercise the persisted checkpoint.
    process.kill()
    process.wait(timeout=5)
    time.sleep(0.2)


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def note_count(value):
    return sum(len(line.get("notes") or []) for line in value["judgeLineList"])


def checkpoint_count():
    envelope = read_json(WORKSPACE / "recovery/session.json")
    return note_count(envelope["document"]) if envelope else None


def capture(filename):
    """Write an X11 PNG using only libX11 and Python's standard library."""
    class XImage(C.Structure):
        _fields_ = [(name, C.c_int) for name in ("width", "height", "xoffset", "format")] + [
            ("data", C.c_void_p),
        ] + [(name, C.c_int) for name in ("byte_order", "bitmap_unit", "bitmap_bit_order", "bitmap_pad", "depth", "bytes_per_line", "bits_per_pixel")] + [
            ("red_mask", C.c_ulong), ("green_mask", C.c_ulong), ("blue_mask", C.c_ulong),
        ]
    lib = C.CDLL(ctypes.util.find_library("X11"))
    lib.XOpenDisplay.argtypes = [C.c_char_p]
    lib.XOpenDisplay.restype = C.c_void_p
    lib.XDefaultRootWindow.argtypes = [C.c_void_p]
    lib.XDefaultRootWindow.restype = C.c_ulong
    lib.XGetImage.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int]
    lib.XGetImage.restype = C.POINTER(XImage)
    lib.XDestroyImage.argtypes = [C.POINTER(XImage)]
    lib.XCloseDisplay.argtypes = [C.c_void_p]
    display = lib.XOpenDisplay(None)
    assert display, "X11 capture requires a display"
    image = lib.XGetImage(display, int(window), 0, 0, 1280, 800, C.c_ulong(-1).value, 2)
    assert image, "XGetImage failed"
    try:
        info = image.contents
        assert info.bits_per_pixel == 32 and info.byte_order == 0 and info.red_mask == 0xff0000, "Unexpected X11 pixel format"
        raw = C.string_at(info.data, info.bytes_per_line * info.height)
        painted = any(raw[i] > 20 for i in range(0, len(raw), 4096))
        rows = bytearray()
        for y in range(info.height):
            source = raw[y * info.bytes_per_line:y * info.bytes_per_line + info.width * 4]
            row = bytearray(len(source))
            row[0::4] = source[2::4]
            row[1::4] = source[1::4]
            row[2::4] = source[0::4]
            row[3::4] = b"\xff" * info.width
            rows.extend(b"\0" + row)
        def chunk(kind, value):
            return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))
        (OUTPUT / filename).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", info.width, info.height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    finally:
        lib.XDestroyImage(image)
        lib.XCloseDisplay(display)
    return painted


def inspect(message):
    if "--inspect" in sys.argv:
        print(message, flush=True)
        input()


try:
    demo = json.loads((ROOT / "assets/charts/phistudio-demo.rpe.json").read_text())
    for folder, title in [("legacy-demo", "Legacy import"), ("second-project", "Second project")]:
        directory = WORKSPACE / "projects" / folder
        directory.mkdir(parents=True)
        value = json.loads(json.dumps(demo))
        value["META"]["name"] = title
        (directory / "chart.json").write_text(json.dumps(value))
        if folder == "legacy-demo":
            def chunk(kind, data):
                return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
            pixels = b"".join(b"\0" + b"".join(bytes((25 + x, 60 + y, 190)) for x in range(168)) for y in range(104))
            cover = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 168, 104, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")
            (directory / "cover.png").write_bytes(cover)
            (directory / "info.yml").write_text("chart: chart.json\nillustration: cover.png\n")

    audio_source = OUTPUT / "wizard-audio.wav"
    with wave.open(str(audio_source), "wb") as audio:
        audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(48000)
        audio.writeframes(b"".join(struct.pack("<h", int(1500 * math.sin(2 * math.pi * 440 * n / 48000))) for n in range(48000)))
    illustration_source = OUTPUT / "wizard-cover.png"
    illustration_source.write_bytes((WORKSPACE / "projects/legacy-demo/cover.png").read_bytes())
    start(1)
    def thumbnail_ready():
        capture("projects-home.png")
        data = (OUTPUT / "projects-home.png").read_bytes()
        compressed = b""
        offset = 8
        while offset < len(data):
            size = struct.unpack(">I", data[offset:offset+4])[0]
            if data[offset+4:offset+8] == b"IDAT": compressed += data[offset+8:offset+8+size]
            offset += size + 12
        raw = zlib.decompress(compressed)
        pixel = raw[150 * (1280*4+1) + 1 + 50*4:150 * (1280*4+1) + 1 + 50*4 + 3]
        return pixel[0] < 180 and 180 <= pixel[2] <= 200
    wait_for(thumbnail_ready, "Project illustration did not become a visible thumbnail")
    inspect("Projects home captured; continue to open first project.")
    click(300, 157)
    location = wait_for(lambda: read_json(WORKSPACE / "last-project.json"), "Home did not open a private project")
    chart = Path(location["chart"])
    assert "legacy-demo" in str(chart), location
    assert note_count(read_json(chart)) == 4
    click(78, 756)
    click(400, 512)
    wait_for(lambda: checkpoint_count() == 5, "Unsaved edit did not reach recovery")
    assert note_count(read_json(chart)) == 4, "Checkpoint overwrote the saved chart"
    stop()
    start(2)
    capture("recovery-prompt.png")
    inspect("Recovery prompt captured; continue to restore.")
    click(500, 459)
    click(78, 756)
    click(430, 450)
    wait_for(lambda: checkpoint_count() == 6, "Restore did not keep recovered notes editable")
    key("ctrl+s")
    wait_for(lambda: note_count(read_json(chart)) == 6, "Saving the recovered chart failed")
    wait_for(lambda: not (WORKSPACE / "recovery/session.json").exists(), "Save did not clear recovery")
    stop()
    start(3)
    capture("projects-after-restart.png")
    inspect("Restart must show Projects; continue to reopen the saved project.")
    click(300, 157)
    click(78, 756)
    click(380, 450)
    wait_for(lambda: checkpoint_count() == 7, "Reopened saved project lost edits")
    stop()
    start(4)
    capture("discard-prompt.png")
    inspect("Continue to discard recovery.")
    click(654, 459)
    wait_for(lambda: not (WORKSPACE / "recovery/session.json").exists(), "Discard did not clear recovery")
    assert note_count(read_json(chart)) == 6
    capture("projects-before-create.png")
    click(78, 83)
    capture("create-chart.png")
    inspect("Create chart captured; continue to fill fields.")
    click(650, 327)
    key("ctrl+a")
    xdo("type", "--clearmodifiers", "New chart")
    click(520, 509)  # Still disabled until both required media are selected.
    assert len(list((WORKSPACE / "projects").iterdir())) == 2
    click(540, 397)
    capture("audio-picker.png")
    inspect("Audio picker captured; continue to choose the source file.")
    click(620, 415)
    xdo("type", "--clearmodifiers", str(audio_source.relative_to(ROOT)))
    click(520, 446)
    time.sleep(.8)
    capture("audio-selected.png")
    inspect("Audio selected; continue to choose the illustration.")
    click(540, 444)
    capture("illustration-picker.png")
    inspect("Illustration picker captured; continue to choose the file.")
    click(620, 415)
    xdo("type", "--clearmodifiers", str(illustration_source.relative_to(ROOT)))
    click(520, 446)
    time.sleep(.8)
    capture("media-ready.png")
    inspect("Both required files selected; continue to create the chart.")
    click(520, 529)
    wait_for(lambda: len(list((WORKSPACE / "projects").iterdir())) == 3, "Create chart did not publish a project")
    last = wait_for(lambda: (v if (v := read_json(WORKSPACE / "last-project.json")) and "legacy-demo" not in v["chart"] else None), "New chart did not open")
    new_chart = Path(last["chart"])
    assert read_json(new_chart)["META"]["name"] == "New chart"
    assert note_count(read_json(new_chart)) == 0
    capture("new-chart-editor.png")
    # Export from File, then from the full-screen library.
    click(20, 20)
    capture("file-menu.png")
    inspect("File menu captured; continue to export.")
    click(95, 127)
    time.sleep(0.7)
    capture("export-dialog.png")
    inspect("Export dialog captured; continue to choose destination.")
    destination = OUTPUT / "new-chart.pez"
    destination.unlink(missing_ok=True)
    click(620, 425)
    key("ctrl+a")
    xdo("type", "--clearmodifiers", str(destination.relative_to(ROOT)))
    click(520, 456)
    wait_for(destination.exists, "File export did not write a package")
    import zipfile
    with zipfile.ZipFile(destination) as package:
        assert "info.yml" in package.namelist()
        exported = next(json.loads(package.read(name)) for name in package.namelist() if name.endswith(".json"))
        assert exported["META"]["name"] == "New chart"
        assert package.read("audio.wav") == audio_source.read_bytes()
        assert package.read("illustration.png") == illustration_source.read_bytes()
    click(20, 20)
    click(65, 61)
    time.sleep(0.7)
    capture("project-library.png")
    inspect("Project library with new chart captured; continue to export a saved project.")
    click(1130, 231)
    time.sleep(0.7)
    capture("library-export-dialog.png")
    inspect("Saved project export dialog captured; continue to select destination.")
    library_destination = OUTPUT / "library-project.pez"
    library_destination.unlink(missing_ok=True)
    click(620, 425)
    key("ctrl+a")
    xdo("type", "--clearmodifiers", str(library_destination.relative_to(ROOT)))
    click(520, 456)
    wait_for(library_destination.exists, "Library export did not write a package")
    with zipfile.ZipFile(library_destination) as package:
        assert "info.yml" in package.namelist()
    capture("final-projects.png")
    # Remove a different project, then the active one, and undo each operation.
    click(1215, 234)
    capture("delete-other-confirm.png")
    inspect("Delete confirmation captured; continue to move the other project to Trash.")
    click(550, 452)
    wait_for(lambda: not (WORKSPACE / "projects/legacy-demo").exists(), "Delete did not remove the chosen library entry")
    assert new_chart.exists(), "Deleting another project removed the current chart"
    capture("delete-other-done.png")
    inspect("Undo row captured; continue to restore the other project.")
    click(240, 302)
    wait_for(lambda: (WORKSPACE / "projects/legacy-demo").exists(), "Undo did not restore the project")
    click(1215, 161)
    capture("delete-current-confirm.png")
    inspect("Delete current confirmation captured; continue.")
    click(550, 452)
    wait_for(lambda: not new_chart.exists(), "Delete did not remove the active project")
    capture("delete-current-done.png")
    inspect("Current project deleted; continue to Undo.")
    click(240, 302)
    wait_for(new_chart.exists, "Undo did not restore the active project's files")
    assert (new_chart.parent / "audio.wav").read_bytes() == audio_source.read_bytes()
    assert (new_chart.parent / "illustration.png").read_bytes() == illustration_source.read_bytes()
    click(300, 161)
    capture("restored-project-editor.png")
    click(80, 20)
    capture("windows-menu.png")
    inspect("Windows menu captured; continue to Settings.")
    click(120, 168)
    capture("settings-window.png")
    inspect("Settings window captured; continue to change and persist audio levels.")
    click(100, 140)
    click(100, 177)
    time.sleep(1.2)
    capture("settings-updated.png")
    settings = wait_for(lambda: read_json(WORKSPACE / "editor-settings.json"), "Preferences did not reach private storage")
    assert settings["music_volume"] < .5 and settings["hitsound_volume"] < .5, settings
    stop()
    start(5)
    assert not (WORKSPACE / "recovery/session.json").exists()
    assert read_json(WORKSPACE / "editor-settings.json") == settings
    capture("final-restart.png")
    click(300, 161)
    click(80, 20)
    click(120, 168)
    capture("settings-after-restart.png")
    (OUTPUT / "result.txt").write_text("PASS\nFullscreen Projects on every normal startup\nLibrary opening preserves private paths\nReal package cover pixels visible; missing art has a placeholder\nProcess-death Restore/Discard preserve saved data\nCreate chart requires valid audio and illustration, preserves their bytes and opens the editor\nPackage export from File and Projects\nDelete and Undo preserve active data and restore project assets\nAudio preferences survive restart\nWorkspace: " + str(WORKSPACE) + "\n")
    print("Project, cover and recovery input smoke PASS:", OUTPUT)
finally:
    if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=5)
    for log in logs:
        log.close()
