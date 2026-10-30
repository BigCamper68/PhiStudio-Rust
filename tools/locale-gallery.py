#!/usr/bin/env python3
"""Inspect persisted locales, line-state popups and Settings on phone/iPad sizes.

Run under private Xvfb >=3200x2048 after building phistudio-app. Each case owns
its source chart and workspace. Catalogue/selector completeness is tested in Rust.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from x11_utils import capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--locales', nargs='+', default=None)
parser.add_argument('--output', type=Path, default=ROOT/'artifacts/checks/locale-gallery')
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
languages = args.locales or [p.stem for p in sorted((ROOT/'assets/locales').glob('*.json')) if p.stem != 'en']
# File-caption widths differ. Settings stays the fourth row of the Windows menu.
menu_x = {'zh-CN': 80, 'zh-TW': 80, 'ja': 117, 'ko': 60, 'ru': 80, 'es': 108}
reports = []


def xdo(*values):
    return subprocess.check_output(['xdotool', *map(str, values)], text=True).strip()


for language in languages:
    assert (ROOT/'assets/locales'/f'{language}.json').is_file(), language
    for name, size, density in [('phone', (873, 393), 2.75), ('ipad', (1194, 834), 2.)]:
        case = Path(tempfile.mkdtemp(prefix=language+'-'+name+'-', dir=out))
        store = case/'store'
        store.mkdir()
        settings = store/'editor-settings.json'
        settings.write_text(json.dumps({'version': 1, 'options': {'language': language}}))
        chart = case/'User Settings 中文 日本語 한국어 Русский Español.json'
        original = json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())
        original['META']['name'] = 'Settings 中文 日本語 한국어 Русский Español'
        chart.write_text(json.dumps(original, ensure_ascii=False))
        digest = hashlib.sha256(chart.read_bytes()).hexdigest()
        physical = tuple(round(value*density) for value in size)
        env = dict(os.environ, PHISTUDIO_TEST_DISPLAY_DENSITY=str(density))
        with (case/'editor.log').open('w') as log:
            process = subprocess.Popen([ROOT/'target/debug/phistudio', '--chart', chart,
                '--workspace', store, '--size', f'{physical[0]}x{physical[1]}'],
                cwd=ROOT, env=env, stdout=log, stderr=log)
            window = None
            try:
                deadline = time.monotonic()+25
                while time.monotonic() < deadline and process.poll() is None:
                    found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name',
                        '^PhiStudio Rust$'], capture_output=True, text=True)
                    if found.returncode == 0:
                        window = found.stdout.splitlines()[-1]
                        break
                    time.sleep(.05)
                assert window
                xdo('windowfocus', '--sync', window)
                wait_for_editor_paint(window, process, height=physical[1])

                def click(x, y):
                    xdo('mousemove', '--sync', round(x*density), round(y*density))
                    xdo('click', 1)
                    time.sleep(.3)

                def capture(label):
                    xdo('mousemove', '--sync', round(500*density), round(50*density))
                    time.sleep(.1)
                    capture_window(window, case/(label+'.png'), *physical)

                capture('workspace')
                sidebar = min(max(size[0]*.30, 210.), 330., size[0]*.4)
                preview_height = min((sidebar-25.)/(16./9.), min(max(size[1]*.4, 90.), 240.))
                click(size[0]-sidebar+80., 148.+preview_height)
                capture('line-states')
                xdo('key', '--clearmodifiers', 'Escape')
                time.sleep(.2)
                selector_x = (size[0]-sidebar)*.54+40.
                click(selector_x, 62)
                click(selector_x, 136)
                capture('storyboard')
                click(menu_x[language], 20)
                capture('windows-menu')
                click(menu_x[language]+30, 168)
                capture('settings')
                click(160, 125)
                capture('languages')
                click(160, 163)  # English, first item of a freshly opened popup.
                deadline = time.monotonic()+8
                while time.monotonic() < deadline:
                    assert process.poll() is None
                    if json.loads(settings.read_text())['options']['language'] == 'en':
                        break
                    time.sleep(.05)
                else:
                    raise AssertionError(f'{language}: Settings could not switch to English')
                capture('english-restored')
                assert hashlib.sha256(chart.read_bytes()).hexdigest() == digest
                reports.append(dict(locale=language, viewport=name, physical=physical,
                    persisted_locale_loaded=True, english_switch_saved=True,
                    line_states_captured=True, storyboard_captured=True,
                    source_unchanged=True, evidence=case.name))
                print('Locale input/captures PASS:', language, name, flush=True)
            except BaseException:
                if window and process.poll() is None:
                    capture_window(window, case/'failure.png', *physical)
                raise
            finally:
                process.kill()
                process.wait(timeout=5)
(out/'report.json').write_text(json.dumps(reports, indent=2)+'\n')
