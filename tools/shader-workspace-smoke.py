#!/usr/bin/env python3
"""Exercise vertical shader tracks with real input on a private 1280x800 Xvfb."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from x11_utils import canvas_right, capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--density', type=float, default=1.)
parser.add_argument('--compact', action='store_true')
parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/checks/shader-workspace-input')
args = parser.parse_args()
W, H = (873, 393) if args.compact else (1280, 800)
DENSITY = args.density
PHYSICAL = (round(W*DENSITY), round(H*DENSITY))
OUT = args.output.resolve()
sidebar = min(330., max(210., W*.3))
main = W-sidebar
full_width = main-52.
split = .54
left = 38.+full_width*split+6.
x = (38.+main-14.)*.5
baseline = H-88.
span, move, resize = (1.5, .5, .5) if args.compact else (4., 1., 1.)
tool_y = H-44.
selector_x = left+50.
OUT.mkdir(parents=True, exist_ok=True)
chart = OUT / 'chart.json'
shutil.copy2(ROOT / 'assets/charts/phistudio-demo.rpe.json', chart)
original = json.loads(chart.read_text())


def xdo(*args):
    return subprocess.check_output(['xdotool', *map(str, args)], text=True).strip()


def click(x, y):
    xdo('mousemove', '--sync', round(x*DENSITY), round(y*DENSITY))
    xdo('click', 1)
    time.sleep(.25)


def drag(x, y, end_x, end_y):
    xdo('mousemove', '--sync', round(x*DENSITY), round(y*DENSITY))
    xdo('mousedown', 1)
    for n in range(1, 11):
        xdo('mousemove', '--sync', round((x + (end_x-x)*n/10)*DENSITY), round((y + (end_y-y)*n/10)*DENSITY))
        time.sleep(.04)
    xdo('mouseup', 1)
    time.sleep(.25)


def key(keys):
    xdo('key', '--clearmodifiers', keys)
    time.sleep(.25)


def save_until(predicate):
    key('ctrl+s')
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        assert process.poll() is None, 'Editor exited'
        document = json.loads(chart.read_text())
        if predicate(document):
            assert document['judgeLineList'] == original['judgeLineList'], 'Shader action changed notes/events'
            return document
        time.sleep(.1)
    raise AssertionError(f'Unexpected saved shader state: {document.get("phiStudioExtra")}')


def shot(name):
    capture_window(window, OUT / name, *PHYSICAL)


with (OUT / 'editor.log').open('w') as log:
    process = subprocess.Popen([ROOT / 'target/debug/phistudio', '--chart', chart, '--size', f'{PHYSICAL[0]}x{PHYSICAL[1]}'], cwd=ROOT, env=dict(os.environ, PHISTUDIO_TEST_DISPLAY_DENSITY=str(DENSITY)), stdout=log, stderr=log)
    window = None
    try:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline and process.poll() is None:
            found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', '^PhiStudio Rust$'], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(.1)
        assert window, 'Editor window did not open'
        xdo('windowfocus', '--sync', window)
        wait_for_editor_paint(window, process, height=PHYSICAL[1])
        shot('events-before.png')
        click(selector_x, 62)
        shot('selector.png')
        click(selector_x, 170)  # Shaders is the last entry of the ordinary selector.
        shot('empty-vertical.png')
        click(99, tool_y)  # The single + Shader placement tool.
        drag(x, baseline, x, baseline-span*96.)  # Draw 0–4 beats upwards.
        save_until(lambda d: len(d.get('phiStudioExtra', {}).get('effects', [])) == 1)
        shot('workspace.png')
        main=canvas_right(OUT/'workspace.png',DENSITY)
        click(30, tool_y)  # Select, as in Events/Storyboard.
        drag(x, baseline-span*48., x, baseline-span*48.-move*96.)  # Move the selected interval by one beat.
        save_until(lambda d: d['phiStudioExtra']['effects'][0]['start'] == ([0, 1, 2] if args.compact else [1, 0, 1]))
        shot('moved.png')
        key('ctrl+z')
        save_until(lambda d: d['phiStudioExtra']['effects'][0]['start'] == [0, 0, 1])
        drag(x, baseline-span*96., x, baseline-(span+resize)*96.)  # Resize the upper endpoint to beat 5.
        save_until(lambda d: d['phiStudioExtra']['effects'][0]['end'] == ([2, 0, 1] if args.compact else [5, 0, 1]))
        key('ctrl+z')
        save_until(lambda d: d['phiStudioExtra']['effects'][0]['end'] == ([1, 1, 2] if args.compact else [4, 0, 1]))
        widen = 40. if args.compact else 70.
        before=main
        drag(main-1., H*.5, main-1.-widen, H*.5)
        main -= widen  # Resizable inspector narrows both timeline halves.
        shot('sidebar-widened.png')
        assert canvas_right(OUT/'sidebar-widened.png',DENSITY)<before-widen*.6,'Inspector resize did not reflow the canvas'
        key('ctrl+d')
        save_until(lambda d: len(d['phiStudioExtra']['effects']) == 2)
        key('Delete')
        save_until(lambda d: len(d['phiStudioExtra']['effects']) == 1)
        # Return through the same selector. Coordinates move with the divider/sidebar.
        selector_x = 88.
        click(selector_x, 62)
        shot('return-selector.png')
        click(selector_x, 100)
        shot('returned.png')
        # A note placement verifies that Events really reopened; Undo restores it.
        click(78, tool_y)
        click(200, baseline-72.)
        key('ctrl+s')
        deadline = time.monotonic()+8
        before_count=len(original['judgeLineList'][0]['notes'])
        while time.monotonic()<deadline:
            if len(json.loads(chart.read_text())['judgeLineList'][0]['notes'])==before_count+1:break
            time.sleep(.1)
        else:raise AssertionError('Returning to Events did not restore the note tool')
        key('ctrl+z')
        save_until(lambda d:d['judgeLineList']==original['judgeLineList'])
        (OUT / 'result.txt').write_text('PASS: Events/Shaders selector, one creation tool, vertical create/move/resize, single-step Undo, duplicate/delete, full-width shader grid and inspector reflow, save and untouched notes/events\n')
        print((OUT / 'result.txt').read_text().strip(), flush=True)
    except BaseException:
        if window and process.poll() is None:
            shot('failure.png')
        raise
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=5)
