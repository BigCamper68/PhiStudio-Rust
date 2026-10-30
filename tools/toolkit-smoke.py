#!/usr/bin/env python3
"""Drive the real editor's selection, clipboard, group gestures and Tools UI.

Run under a private 1280x800 Xvfb. Only this probe's fixtures/workspace change.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import struct
import zlib
import time
from x11_utils import capture_window, wait_for_editor_paint

ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--forms-only',action='store_true')
parser.add_argument('--compact',action='store_true')
args=parser.parse_args()
WIDTH,HEIGHT=(873,393) if args.compact else (1280,800)
OUT=ROOT/('artifacts/checks/toolkit-compact-0.11' if args.compact else 'artifacts/checks/toolkit-input-0.11')
OUT.mkdir(parents=True,exist_ok=True)
workspace=Path(tempfile.mkdtemp(prefix='workspace-',dir=OUT))
chart=workspace/'tools.rpe.json'
root=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())
root['META'].update(name='Tools input probe',song='',background='',offset=0)
root['BPMList']=[dict(bpm=120.,startTime=[0,0,1])]
line=root['judgeLineList'][0]
root['judgeLineList']=[line]
line['notes']=[dict(type=1,startTime=[i,0,1],endTime=[i,0,1],positionX=x,above=1,isFake=0,alpha=255,size=1.,speed=1.,visibleTime=999999.,yOffset=0.,customToolFixture=f'n{i}') for i,x in enumerate([-270.,0.,270.],1)]
line['numOfNotes']=3
layer={}
for track,value in [('moveXEvents',0.),('moveYEvents',0.),('rotateEvents',0.),('alphaEvents',255.),('speedEvents',10.)]:
    layer[track]=[dict(startTime=[0,0,1],endTime=[1,0,1],start=value,end=value,easingType=1,easingLeft=0.,easingRight=1.,bezier=0,bezierPoints=[0.,0.,1.,1.],linkgroup=0)]
line['eventLayers']=[layer]
chart.write_text(json.dumps(root))
original=json.loads(chart.read_text())

def xdo(*args): return subprocess.check_output(['xdotool',*map(str,args)],text=True).strip()
def key(keys): xdo('key','--clearmodifiers',keys);time.sleep(.18)
def click(x,y): xdo('mousemove','--sync',x,y);xdo('click','1');time.sleep(.2)
def drag(a,b):
    xdo('mousemove','--sync',*a);xdo('mousedown','1')
    for step in range(1,13):
        xdo('mousemove','--sync',round(a[0]+(b[0]-a[0])*step/12),round(a[1]+(b[1]-a[1])*step/12));time.sleep(.02)
    xdo('mouseup','1');time.sleep(.2)
def saved(predicate,label):
    key('ctrl+s');deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        assert process.poll() is None,'Editor exited during '+label
        document=json.loads(chart.read_text());rows=document['judgeLineList'][0]['notes']
        if predicate(rows): return rows
        time.sleep(.05)
    raise AssertionError(f'{label}: {rows}')
def count(n): return saved(lambda rows:len(rows)==n,f'{n} notes')
def screenshot(name): capture_window(window,OUT/(name+'.png'),WIDTH,HEIGHT)

def wheel(button, repeats=18, x=350, y=None):
    if y is None:y=min(400,HEIGHT-90)
    xdo('mousemove','--sync',x,y);xdo('click','--repeat',repeats,'--delay',12,button);time.sleep(.3)
def choose_page(index):
    click(240,125)
    wheel(4,8,210,220)
    if index>=5:
        wheel(5,14,210,220);click(210,332-37*(9-index))
    else:click(210,163+37*index)
    wheel(4)
def last_button(name):
    wheel(5,65);screenshot(name)
    data=(OUT/(name+'.png')).read_bytes();offset=8;compressed=bytearray()
    while offset<len(data):
        length=struct.unpack('>I',data[offset:offset+4])[0];kind=data[offset+4:offset+8]
        if kind==b'IDAT':compressed.extend(data[offset+8:offset+8+length])
        offset+=12+length
    raw=zlib.decompress(compressed);stride=WIDTH*3+1
    raised=[];run=None
    for y in range(205,HEIGHT-(8 if args.compact else 100)):
        color=raw[y*stride+1+62*3:y*stride+1+62*3+3]
        if color==bytes((29,39,57)):
            if run is None:run=y
        elif run is not None:
            if y-run>=22:raised.append((run,y))
            run=None
    assert raised, 'No reachable action button in '+name
    top,bottom=raised[-1];click(100,(top+bottom)//2)
def saved_events(expected,label):
    key('ctrl+s');deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        assert process.poll() is None,'Editor exited during '+label
        document=json.loads(chart.read_text());layer=document['judgeLineList'][0]['eventLayers'][0]
        if sum(len(layer.get(k,[])) for k in ['moveXEvents','moveYEvents','rotateEvents','alphaEvents','speedEvents'])==expected:return layer
        time.sleep(.05)
    raise AssertionError(label+': '+str(layer))

with (OUT/'editor.log').open('w') as log:
    process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--size',f'{WIDTH}x{HEIGHT}'],cwd=ROOT,stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+20;window=None
        while time.monotonic()<deadline and process.poll() is None:
            found=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
            if found.returncode==0:window=found.stdout.splitlines()[-1];break
            time.sleep(.1)
        assert window,'Editor window did not open'
        xdo('windowfocus','--sync',window);wait_for_editor_paint(window,process,height=HEIGHT)
        if not args.forms_only and not args.compact:
            key('ctrl+a');key('ctrl+c');key('m')
            rows=saved(lambda n:[v['positionX'] for v in n]==[270.,0.,-270.],'Mirror X')
            assert [n['customToolFixture'] for n in rows]==['n1','n2','n3']
            key('ctrl+z');rows=count(3);assert rows==original['judgeLineList'][0]['notes']
            key('ctrl+a');key('r');saved(lambda n:[v['startTime'][0] for v in n]==[3,2,1],'Reverse time')
            key('ctrl+z');key('ctrl+a');key('ctrl+d');count(6);key('ctrl+z');count(3)
            key('ctrl+a');key('ctrl+x');count(0);key('ctrl+z');count(3)
            key('ctrl+v');rows=count(6);assert rows[3]['startTime']==[0,0,1]
            key('ctrl+z');key('ctrl+shift+v');rows=count(6);assert rows[3]['positionX']==270.
            key('ctrl+z');key('ctrl+a')
            drag((181,608),(229,512))
            saved(lambda n:[v['startTime'][0] for v in n]==[2,3,4] and [v['positionX'] for v in n]==[-135.,135.,405.],'Group move')
            screenshot('group-move');key('ctrl+z');count(3)
            key('b');drag((150,390),(350,625));key('Delete');rows=count(1);assert rows[0]['customToolFixture']=='n3'
            key('ctrl+z');count(3)
            key('e');drag((181,608),(277,512));rows=count(1);assert rows[0]['customToolFixture']=='n3'
            key('ctrl+z');count(3);key('v');key('ctrl+a')
        key('ctrl+a')
        click(515,HEIGHT-44);screenshot('tools-selection')
        click(245,125);screenshot('tool-groups-menu')
        if not args.compact:
            # The page popup is already open in the menu screenshot.
            click(210,274);wheel(4);screenshot('note-generator-top')
            last_button('note-generator-ready');count(20);screenshot('generated-notes')
            key('ctrl+z');count(3)
            choose_page(0);click(165,227);click(355,179);saved_events(0,'Delete all event tracks')
            choose_page(5);screenshot('motion-generator-top');last_button('motion-generator-ready')
            layer=saved_events(112,'Sampled motion generation');assert len(layer['moveXEvents'])==56 and len(layer['moveYEvents'])==56
            screenshot('generated-motion');key('ctrl+z');saved_events(0,'Undo motion');key('ctrl+z');saved_events(5,'Restore original events')
            choose_page(0);click(165,227);choose_page(4);screenshot('event-tools')
            click(170,424);saved_events(10,'Subdivide five event tracks');screenshot('subdivided-events')
            # Select the two Rotate intervals while the Tools window stays open.
            key('b');drag((700,590),(766,715));click(160,461)
            saved_events(9,'Join the selected linear Rotate pair');key('ctrl+z');saved_events(10,'Undo join');key('ctrl+z');saved_events(5,'Undo subdivision')
            choose_page(0);click(90,227);choose_page(2);screenshot('batch-values')
            last_button('batch-values-ready');saved(lambda n:[v['positionX'] for v in n]==[-450.,0.,450.],'Apply batch ramp')
            key('ctrl+z');count(3);choose_page(1);screenshot('transform-arrange')
        else:
            click(210,274);wheel(4);last_button('compact-note-generator');count(20)
            key('ctrl+z');count(3)
            choose_page(0);click(165,227);click(355,179);saved_events(0,'Compact delete events')
            choose_page(5);last_button('compact-motion-generator');saved_events(112,'Compact motion generation')
            screenshot('compact-generated-motion');key('ctrl+z');saved_events(0,'Compact undo motion');key('ctrl+z');saved_events(5,'Compact restore events')
            for page in range(6):choose_page(page);screenshot('compact-group-'+str(page))
        (OUT/'result.txt').write_text('PASS: real shortcuts, raw fields, group drag, Box, Erase, generators, event subdivision/join and one-step undo\n')
        print('Toolkit input smoke PASS:',OUT)
    except Exception:
        if window and process.poll() is None: screenshot('failure')
        raise
    finally:
        process.terminate()
        try:process.wait(timeout=8)
        except subprocess.TimeoutExpired:process.kill();process.wait(timeout=8)
