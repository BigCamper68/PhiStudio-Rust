#!/usr/bin/env python3
"""Exercise whole-song/Notes seeking and Unicode UI with physical X11 input."""
import argparse
import json
from pathlib import Path
import subprocess
import time
import wave
from x11_utils import capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=ROOT/'artifacts/checks/header-seek')
args = parser.parse_args()
out = args.output.resolve()
assets = out/'assets'
assets.mkdir(parents=True, exist_ok=True)
chart = assets/'chart.json'
original = json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())
original['META']['name'] = 'Русский Ⅱ ↔ 中文 日本語 한글 ไทย 😀 🎵'
chart.write_text(json.dumps(original, ensure_ascii=False))
with wave.open(str(assets/'tail.wav'), 'wb') as audio:
    audio.setparams((1, 2, 8000, 0, 'NONE', 'not compressed'))
    audio.writeframes(bytes(180*8000*2))
(out/'.phistudio-workspace.json').write_text(json.dumps({
    'phistudio_workspace': 1, 'source_name': 'Unicode seek fixture', 'assets': 'assets',
    'charts': [{'source': 'assets/chart.json', 'working': 'assets/chart.json',
                'format': 'rpe', 'options': {'music': 'tail.wav'}}], 'suggested': 0,
}))

def xdo(*args):
    return subprocess.check_output(['xdotool', *map(str, args)], text=True).strip()

def click(x, y):
    xdo('mousemove', '--sync', x, y)
    xdo('click', 1)
    time.sleep(.25)

def key(value):
    xdo('key', '--clearmodifiers', value)
    time.sleep(.25)

def saved(count):
    key('ctrl+s')
    deadline = time.monotonic()+8
    while time.monotonic() < deadline:
        doc = json.loads(chart.read_text())
        if len(doc['judgeLineList'][0]['notes']) == count:
            return doc
        assert process.poll() is None, 'Editor exited'
        time.sleep(.05)
    raise AssertionError(f'Expected {count} notes')

def last_note_beat(doc):
    def beat(row):
        a, b, c = row['startTime']
        return a+b/c
    return max(map(beat, doc['judgeLineList'][0]['notes']))

with (out/'editor.log').open('w') as log:
    process = subprocess.Popen([ROOT/'target/debug/phistudio', '--chart', chart,
                                '--size', '1280x800'], cwd=ROOT, stdout=log, stderr=log)
    window = None
    try:
        deadline = time.monotonic()+25
        while process.poll() is None and time.monotonic() < deadline:
            found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', '^PhiStudio Rust$'], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(.05)
        assert window, 'No editor window'
        xdo('windowfocus', '--sync', window)
        wait_for_editor_paint(window, process)
        time.sleep(.25)
        capture_window(window, out/'unicode-header.png')
        count = len(original['judgeLineList'][0]['notes'])
        click(78, 756)  # Tap tool.
        click(1266, 20)  # End of the whole-song seek bar, default Notes off.
        click(200, 711)
        doc = saved(count+1)
        global_beat = last_note_beat(doc)
        assert 440 < global_beat <= 451, f'Whole-song seek stopped at beat {global_beat}'
        capture_window(window, out/'whole-song-end.png')
        key('ctrl+z')
        assert saved(count) == original
        click(980, 20)  # Notes toggle immediately before the seek bar.
        click(1266, 20)
        click(200, 711)
        doc = saved(count+1)
        notes_beat = last_note_beat(doc)
        assert 60 < notes_beat <= 65, f'Notes range did not replace song range: {notes_beat}'
        capture_window(window, out/'notes-range.png')
        key('ctrl+z')
        assert saved(count) == original
        click(980, 20)  # Restore whole-song mode.
        click(1266, 20)
        click(200, 711)
        assert 440 < last_note_beat(saved(count+1)) <= 451
        key('ctrl+z')
        assert saved(count) == original
        result = {'result': 'PASS', 'song_seconds': 180, 'global_beat': global_beat,
                  'notes_beat': notes_beat, 'original_chart_restored': True}
        (out/'report.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result), flush=True)
    except BaseException:
        if window and process.poll() is None:
            capture_window(window, out/'failure.png')
        raise
    finally:
        process.kill()
        process.wait(timeout=5)
