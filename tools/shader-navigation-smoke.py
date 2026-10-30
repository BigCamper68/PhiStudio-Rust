#!/usr/bin/env python3
"""Exercise Java-style shader parameter navigation with real X11 input."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from x11_utils import canvas_right, capture_window, wait_for_editor_paint

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--density', type=float, default=1.)
parser.add_argument('--compact', action='store_true')
parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/checks/shader-navigation-input')
args = parser.parse_args()
OUT = args.output.resolve()
OUT.mkdir(parents=True, exist_ok=True)
W, H = (873, 393) if args.compact else (1280, 800)
physical = (round(W*args.density), round(H*args.density))
sidebar = min(330., max(210., W*.3))
main = W-sidebar
baseline = H-88.
span = 1.5 if args.compact else 4.
# Shared sidebar reveals the properties on compact screens.
scalar_y = 164. if args.compact else 364.
apply_y = 200. if args.compact else 400.
color_y = 207. if args.compact else 419.
chart = OUT/'chart.json'
original = json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())
original['phiStudioExtra'] = {'bpm':150, 'effects':[{'start':[0,0,1], 'end':[4,0,1], 'shader':'vignette', 'vars':{'color':[0,0,0,255], 'radius':15.}, 'custom':'preserved'}]}
chart.write_text(json.dumps(original))


def xdo(*args):
    return subprocess.check_output(['xdotool', *map(str,args)],text=True).strip()


def click(x,y):
    xdo('mousemove','--sync',round(x*args.density),round(y*args.density))
    xdo('click',1)
    time.sleep(.25)


def key(keys):
    xdo('key','--clearmodifiers',keys)
    time.sleep(.25)


def drag(x,y,end_x,end_y):
    xdo('mousemove','--sync',round(x*args.density),round(y*args.density))
    xdo('mousedown',1)
    for i in range(1,11):
        xdo('mousemove','--sync',round((x+(end_x-x)*i/10)*args.density),round((y+(end_y-y)*i/10)*args.density))
        time.sleep(.04)
    xdo('mouseup',1)
    time.sleep(.25)


def shot(name):
    capture_window(window,OUT/(name+'.png'),*physical)


def save_until(predicate, notes_unchanged=True):
    key('ctrl+s')
    deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        assert process.poll() is None,'Editor exited'
        doc=json.loads(chart.read_text())
        if predicate(doc):
            if notes_unchanged:
                assert doc['judgeLineList']==original['judgeLineList'],'Shader UI modified notes/events'
            assert doc['phiStudioExtra']['effects'][0]['custom']=='preserved'
            return doc
        time.sleep(.1)
    raise AssertionError(f'Unexpected shader state: {doc["phiStudioExtra"]}')


with (OUT/'editor.log').open('w') as log:
    process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--size',f'{physical[0]}x{physical[1]}'],cwd=ROOT,env=dict(os.environ,PHISTUDIO_TEST_DISPLAY_DENSITY=str(args.density)),stdout=log,stderr=log)
    window=None
    try:
        deadline=time.monotonic()+30
        while time.monotonic()<deadline and process.poll() is None:
            found=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
            if found.returncode==0:
                window=found.stdout.splitlines()[-1]
                break
            time.sleep(.1)
        assert window,'No editor window'
        xdo('windowfocus','--sync',window)
        wait_for_editor_paint(window,process,height=physical[1])
        old_selector=38.+(main-52.)*.54+56.
        click(old_selector,62)
        click(old_selector,170)
        click((38.+main-14.)*.5,baseline-72.)
        shot('effect')
        main=canvas_right(OUT/'effect.png',args.density)
        click(132,86)  # Parameters page, color selected first.
        shot('color')
        click(365,86)
        shot('parameter-list')
        click(365,190)  # Radius.
        shot('radius')
        save_until(lambda d:d['phiStudioExtra']==original['phiStudioExtra'])
        # Edit an actual scalar and apply it; the form is separate from animation.
        click(main+30.,scalar_y)
        key('ctrl+a')
        xdo('type','--clearmodifiers','7')
        key('Return')
        click(main+45.,apply_y)
        save_until(lambda d:d['phiStudioExtra']['effects'][0]['vars']['radius']==7.)
        shot('radius-applied')
        click(190,60)  # Back to effect is always visible.
        click(215,86)  # Animation, same selected radius parameter.
        shot('animation-empty')
        click(100,H-44.)  # The placement tool is now + Keyframe.
        x=(38.+main-14.)*.5
        drag(x,baseline,x,baseline-span*96.)
        save_until(lambda d:isinstance(d['phiStudioExtra']['effects'][0]['vars']['radius'],list))
        doc=json.loads(chart.read_text())
        row=doc['phiStudioExtra']['effects'][0]['vars']['radius'][0]
        assert row['start']==row['end']==7.,row
        shot('radius-animation')
        key('ctrl+z')
        save_until(lambda d:d['phiStudioExtra']['effects'][0]['vars']['radius']==7.)
        key('ctrl+shift+z')
        save_until(lambda d:isinstance(d['phiStudioExtra']['effects'][0]['vars']['radius'],list))
        key('ctrl+d')
        save_until(lambda d:len(d['phiStudioExtra']['effects'])==1 and len(d['phiStudioExtra']['effects'][0]['vars']['radius'])==2)
        key('Delete')
        save_until(lambda d:len(d['phiStudioExtra']['effects'])==1 and len(d['phiStudioExtra']['effects'][0]['vars']['radius'])==1)
        click(190,60)
        shot('back-to-effect')
        click(132,86)
        click(365,86)
        click(365,122)  # Color.
        click(main+90.,color_y)
        shot('color-palette')
        key('Escape')  # Close the palette before returning from the parameter.
        shot('color-canceled')
        click(190,60)
        shot('color-back-to-effect')
        save_until(lambda d:d['phiStudioExtra']['effects'][0]['vars']['color']==[0,0,0,255])
        widen=40. if args.compact else 70.
        shot('before-resize')
        resize_edge=canvas_right(OUT/'before-resize.png',args.density)
        drag(resize_edge-1.,H*.5,resize_edge-1.-widen,H*.5)
        shot('inspector-widened')
        assert canvas_right(OUT/'inspector-widened.png',args.density)<resize_edge-widen*.6,'Inspector resize did not reflow the canvas'
        click(88,62)
        click(88,100)
        shot('returned-to-events')
        click(78,H-44.)
        click(200,baseline-72.)
        count=len(original['judgeLineList'][0]['notes'])
        save_until(lambda d:len(d['judgeLineList'][0]['notes'])==count+1,notes_unchanged=False)
        key('ctrl+z')
        save_until(lambda d:d['judgeLineList']==original['judgeLineList'])
        result='PASS: full shader canvas, color/radius selection, scalar Apply, visible Back, first animation from a constant, keyframe Undo/Redo/duplicate/delete, palette cancel, inspector resize, return to Events and untouched notes/events'
        (OUT/'result.txt').write_text(result+'\n')
        print(result,flush=True)
    except BaseException:
        if window and process.poll() is None:
            shot('failure')
        raise
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=5)
