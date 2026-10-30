#!/usr/bin/env python3
"""Switch and persist English/Chinese through physical X11 input on phone/iPad sizes.

Run under a private Xvfb screen at least 3200x2048 after building phistudio-app.
Only newly created test workspaces below --output are modified.
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
parser.add_argument('--output', type=Path, default=ROOT/'artifacts/checks/localization')
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)


def xdo(*args):
    return subprocess.check_output(['xdotool', *map(str, args)], text=True).strip()


reports = []
for name, size, density in [('phone', (873, 393), 2.75),
                            ('ipad-11', (1194, 834), 2.),
                            ('ipad-12.9', (1366, 1024), 2.)]:
    case = Path(tempfile.mkdtemp(prefix=name+'-', dir=out))
    physical = tuple(round(value*density) for value in size)
    chart = case/'Settings 中文 😀.json'
    original = json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())
    original['META']['name'] = 'Settings 中文 😀'
    chart.write_text(json.dumps(original, ensure_ascii=False))
    digest = hashlib.sha256(chart.read_bytes()).hexdigest()
    store = case/'store'
    settings = store/'editor-settings.json'
    env = dict(os.environ, PHISTUDIO_TEST_DISPLAY_DENSITY=str(density))

    def click(x, y):
        xdo('mousemove', '--sync', round(x*density), round(y*density))
        xdo('click', 1)
        time.sleep(.3)

    def capture(label):
        # Move off controls so a tooltip cannot obscure the translated text.
        xdo('mousemove', '--sync', round(500*density), round(50*density))
        time.sleep(.1)
        capture_window(window, case/(label+'.png'), *physical)

    def language_saved(code):
        deadline = time.monotonic()+8
        while time.monotonic() < deadline:
            assert process.poll() is None, 'Editor exited'
            try:
                if json.loads(settings.read_text())['options']['language'] == code:
                    return
            except (OSError, ValueError, KeyError):
                pass
            time.sleep(.05)
        raise AssertionError(f'Language {code} was not persisted')

    for launch in range(2):
        with (case/f'editor-{launch}.log').open('w') as log:
            process = subprocess.Popen([
                ROOT/'target/debug/phistudio', '--chart', chart, '--workspace', store,
                '--size', f'{physical[0]}x{physical[1]}',
            ], cwd=ROOT, env=env, stdout=log, stderr=log)
            window = None
            try:
                deadline = time.monotonic()+25
                while process.poll() is None and time.monotonic() < deadline:
                    found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name',
                                            '^PhiStudio Rust$'], capture_output=True, text=True)
                    if found.returncode == 0:
                        window = found.stdout.splitlines()[-1]
                        break
                    time.sleep(.05)
                assert window, 'No editor window'
                xdo('windowfocus', '--sync', window)
                wait_for_editor_paint(window, process, height=physical[1])
                if launch:
                    language_saved('zh-CN')
                    capture('chinese-after-restart')
                click(80, 20)  # Windows / 窗口
                click(120, 168)  # Settings / 设置
                capture('english-settings' if launch == 0 else 'chinese-settings-restored')
                click(160, 125)  # Language selector stays at the top in both languages.
                capture('language-selector' if launch == 0 else 'language-selector-restored')
                click(160, 200 if launch == 0 else 163)  # Native names: 简体中文 / English.
                language_saved('zh-CN' if launch == 0 else 'en')
                capture('chinese-settings' if launch == 0 else 'english-restored')
                assert hashlib.sha256(chart.read_bytes()).hexdigest() == digest
            except BaseException:
                if window and process.poll() is None:
                    capture('failure')
                raise
            finally:
                # Deliberately skip shutdown callbacks: the saved language must
                # survive process death using the ordinary preference writer.
                process.kill()
                process.wait(timeout=5)
    reports.append(dict(name=name, physical=physical, density=density,
                        chinese_persisted=True, switched_back=True,
                        original_source_unchanged=True, evidence=str(case.relative_to(out))))
    print('Language switching/restart PASS:', name, physical, flush=True)
(out/'report.json').write_text(json.dumps(reports, indent=2)+'\n')
(out/'result.txt').write_text('PASS: physical English → 简体中文, persisted across process death, '
                            'Chinese → English; phone and both iPad Pro M2 screen sizes; '
                            'original source bytes unchanged.\n')
