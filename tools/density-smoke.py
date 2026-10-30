#!/usr/bin/env python3
"""Check actual GL layout and physical pointer input at Android-like density.
Run under a private Xvfb screen at least 3200x1500. Debug build only.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import struct
import zlib
from x11_utils import capture_window, wait_for_editor_paint
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,default=ROOT/'artifacts/checks/density-0.11.1');args=parser.parse_args()
OUT=args.output.resolve();OUT.mkdir(parents=True,exist_ok=True)
def pixel(path,x,y):
    png=path.read_bytes();at=8;parts=[];width=None
    while at<len(png):
        size=struct.unpack('>I',png[at:at+4])[0];kind=png[at+4:at+8];data=png[at+8:at+8+size];at+=size+12
        if kind==b'IHDR':width=struct.unpack('>I',data[:4])[0]
        elif kind==b'IDAT':parts.append(data)
    rows=zlib.decompress(b''.join(parts));offset=y*(1+width*3)+1+x*3
    return tuple(rows[offset:offset+3])
baseline=None
reports=[]
for density in [1.,2.75,3.5]:
    case=OUT/str(density);case.mkdir(exist_ok=True)
    physical=(round(873*density),round(393*density))
    env=dict(os.environ,PHISTUDIO_TEST_DISPLAY_DENSITY=str(density))
    with (case/'smoke.log').open('w') as log:
        subprocess.run([ROOT/'target/debug/phistudio','--size',f'{physical[0]}x{physical[1]}','--smoke-test',case],cwd=ROOT,env=env,stdout=log,stderr=log,check=True,timeout=80)
    assert (case/'smoke-test.txt').read_text().startswith('PASS')
    geometry=json.loads((case/'ui-geometry.json').read_text())
    assert abs(geometry['pixels_per_point']-density)<1e-5,geometry
    assert geometry['framebuffer']==list(physical),geometry
    if baseline is None:baseline=geometry
    for field in ['grid','preview']:
        assert all(abs(a-b)<1. for a,b in zip(geometry[field],baseline[field])),(field,geometry,baseline)
    reports.append(dict(density=density,physical=physical,geometry=geometry))
    # Exercise physical X11 coordinates, including toolbar and chart clicks.
    workspace=Path(tempfile.mkdtemp(prefix='input-',dir=case));chart=workspace/'chart.json'
    original=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text());chart.write_text(json.dumps(original))
    def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True).strip()
    def key(keys):xdo('key','--clearmodifiers',keys);time.sleep(.2)
    def click(x,y):xdo('mousemove','--sync',round(x*density),round(y*density));xdo('click',1);time.sleep(.3)
    def shot(name):capture_window(window,case/(name+'.png'),*physical)
    def wait_pixel(name, expected):
        deadline=time.monotonic()+10
        while True:
            shot(name)
            if pixel(case/(name+'.png'),4,4)==expected:return
            assert process.poll() is None,'Editor exited during view transition'
            assert time.monotonic()<deadline, f'View transition timed out: {name}'
            time.sleep(.05)
    def saved_count(expected):
        key('ctrl+s');deadline=time.monotonic()+8
        while time.monotonic()<deadline:
            assert process.poll() is None,'Editor exited during scaled input'
            rows=json.loads(chart.read_text())['judgeLineList'][0]['notes']
            if len(rows)==expected:return rows
            time.sleep(.05)
        raise AssertionError(f'Density {density}: expected {expected} notes; got {rows}')
    with (case/'input.log').open('w') as log:
        process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--size',f'{physical[0]}x{physical[1]}'],cwd=ROOT,env=env,stdout=log,stderr=log)
        try:
            window=None;deadline=time.monotonic()+20
            while time.monotonic()<deadline and process.poll() is None:
                found=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
                if found.returncode==0:window=found.stdout.splitlines()[-1];break
                time.sleep(.1)
            assert window,'Editor window did not open'
            xdo('windowfocus','--sync',window);wait_for_editor_paint(window,process,height=physical[1])
            shot('input-before');click(78,349);click(280,220);saved_count(5);shot('input-added')
            key('ctrl+z');saved_count(4);key('ctrl+shift+z');saved_count(5)
            # Selection now intentionally reveals Properties, scrolling Preview
            # offscreen. Return the sidebar to its top before opening Preview.
            xdo('mousemove','--sync',round(850*density),round(120*density));xdo('click','--repeat',26,'--delay',12,4);time.sleep(.4)
            shot('sidebar-preview');click(740,140);wait_pixel('fullscreen',(0,0,0))
            click(110,24);wait_pixel('returned',(20,27,42))
            click(80,20);click(120,168);shot('multiple-lines')
            key('ctrl+s');assert len(json.loads(chart.read_text())['judgeLineList'][0]['notes'])==5
        except BaseException:
            if window and process.poll() is None:shot('failure')
            raise
        finally:
            process.kill();process.wait(timeout=5)
    print('Density layout and physical input PASS:',density,physical,flush=True)
(OUT/'report.json').write_text(json.dumps(reports,indent=2)+'\n')
(OUT/'result.txt').write_text('PASS\nSystem densities 1.0/2.75/3.5 retain the same logical grid and preview layout. Real physical toolbar/grid input, add/undo/redo/save, fullscreen return and Multiple lines are exercised.\n')
