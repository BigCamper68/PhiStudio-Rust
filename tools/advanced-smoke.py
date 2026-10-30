#!/usr/bin/env python3
"""Exercise advanced authoring and source-note navigation through real GL/input.
Run under a private 1280x800 Xvfb; generated files stay in artifacts.
"""
import argparse, copy, json, struct, subprocess, tempfile, time, zlib
from pathlib import Path
from x11_utils import capture_window, wait_for_editor_paint
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--compact',action='store_true');args=p.parse_args()
W,H=(873,393) if args.compact else (1280,800)
OUT=ROOT/('artifacts/checks/advanced-compact' if args.compact else 'artifacts/checks/advanced-input');OUT.mkdir(parents=True,exist_ok=True)
workspace=Path(tempfile.mkdtemp(dir=OUT));chart=workspace/'chart.json'
raw=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text());raw['META'].update(name='Advanced input',song='',background='',offset=0);raw['BPMList']=[dict(bpm=120.,startTime=[0,0,1])]
line=raw['judgeLineList'][0];line.update(Name='Source',notes=[],numOfNotes=0)
for track in line['eventLayers'][0].values():
    if isinstance(track,list):
        for e in track:e.update(startTime=[0,0,1],endTime=[1,0,1])
raw['judgeLineList']=[copy.deepcopy(line) for _ in range(4)]
for i,l in enumerate(raw['judgeLineList']):
    l['Name']=['Source','Preview target','Destination A','Destination B'][i]
    if i>=2:l['eventLayers']=[{}]
template=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text())['judgeLineList'][0]['notes'][0]
raw['judgeLineList'][1]['notes']=[dict(template,type=2,startTime=[100,0,1],endTime=[101,0,1],positionX=300.),dict(template,type=1,startTime=[0,1,2],endTime=[0,1,2],positionX=-100.,probeTarget=True)]
raw['judgeLineList'][1]['numOfNotes']=2
chart.write_text(json.dumps(raw));original=copy.deepcopy(raw)
def xdo(*a):return subprocess.check_output(['xdotool',*map(str,a)],text=True).strip()
def key(k):xdo('key','--clearmodifiers',k);time.sleep(.22)
def click(x,y):xdo('mousemove','--sync',round(x),round(y));xdo('click',1);time.sleep(.35)
def wheel(b,n=12,x=300,y=None):
    xdo('mousemove','--sync',x,y or min(400,H-100));xdo('click','--repeat',n,'--delay',15,b);time.sleep(.35)
def drag(a,b):
    xdo('mousemove','--sync',*a);xdo('mousedown',1)
    for i in range(1,13):
        xdo('mousemove','--sync',round(a[0]+(b[0]-a[0])*i/12),round(a[1]+(b[1]-a[1])*i/12));time.sleep(.02)
    xdo('mouseup',1);time.sleep(.3)
def shot(n):
    path=OUT/(n+'.png');capture_window(window,path,W,H);return path
def rgb(path):
    data=path.read_bytes();at=8;parts=[]
    while at<len(data):
        n=struct.unpack('>I',data[at:at+4])[0];kind=data[at+4:at+8]
        if kind==b'IDAT':parts.append(data[at+8:at+8+n])
        at+=n+12
    return zlib.decompress(b''.join(parts))
def last_clone_action():
    wheel(5,18,x=300,y=min(400,H-100))
    data=rgb(shot('clone-ready'));runs=[];start=None
    for y in range(100,H):
        at=y*(W*3+1)+1+50*3;color=data[at:at+3]
        if color==bytes((29,39,57)):
            if start is None:start=y
        elif start is not None:
            if y-start>=22:runs.append((start,y))
            start=None
    assert runs,'Clone action button is unreachable'
    a,b=runs[-1];click(85,(a+b)/2)
def save(predicate,label):
    key('ctrl+s');deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        assert process.poll() is None,'Editor exited: '+label
        data=json.loads(chart.read_text())
        if predicate(data):return data
        time.sleep(.05)
    raise AssertionError(label)
with (OUT/'editor.log').open('w') as log:
    process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--size',f'{W}x{H}'],cwd=ROOT,stdout=log,stderr=log)
    try:
        window=None;deadline=time.monotonic()+25
        while process.poll() is None and time.monotonic()<deadline:
            found=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
            if found.returncode==0:window=found.stdout.splitlines()[-1];break
            time.sleep(.1)
        assert window,'Window did not open';xdo('windowfocus','--sync',window);wait_for_editor_paint(window,process,height=H)
        shot('editor');click(W-130,140);path=shot('fullscreen')
        data=rgb(path);points=[]
        for y in range(60,H-40):
            for x in range(25,W-25):
                at=y*(W*3+1)+1+x*3;r,g,b=data[at:at+3]
                if b>=g and b>200 and g>140 and r<210:points.append((x,y))
        assert len(points)>10,'Native target note was not visible'
        click(*points[len(points)//2]);shot('picked-source-note')
        key('Delete');save(lambda d:len(d['judgeLineList'][1]['notes'])==1 and not d['judgeLineList'][1]['notes'][0].get('probeTarget'),'Delete exact source note selected by Preview')
        key('ctrl+z');save(lambda d:d==original,'Undo source-note deletion')
        click(22,20);shot('file-menu')
        (OUT/'partial.txt').write_text('Native Preview tap/source-line/source-index/delete/Undo PASS\n')
        click(90,180);shot('chart-properties')
        if not args.compact:
            click(110,130);key('ctrl+a');xdo('type','--clearmodifiers','Renamed input');click(125,347)
            save(lambda d:d['META']['name']=='Renamed input','Chart metadata apply')
            key('ctrl+z');save(lambda d:d==original,'Metadata exact Undo')
            click(1090,415);shot('storyboard')
            click(120,317);key('ctrl+a');xdo('type','--clearmodifiers','0.8')
            click(120,360);key('ctrl+a');xdo('type','--clearmodifiers','1.2');click(120,429)
            save(lambda d:d['judgeLineList'][1].get('extended',{}).get('scaleXEvents',[{}])[0].get('start')==0.8,'Storyboard native Scale X')
            shot('storyboard-applied');click(460,70);key('ctrl+z');save(lambda d:d==original,'Storyboard exact Undo')
            click(1100,360);shot('line-picker');click(1090,400)
            click(570,650);key('ctrl+a');click(515,H-44);click(245,125);click(210,311)
            shot('event-tools');click(110,247);shot('clone-events');click(110,230);shot('clone-chooser')
            click(90,272);click(90,309);click(105,384);shot('clone-targets')
            last_clone_action()
            save(lambda d:all(len(d['judgeLineList'][i]['eventLayers'][0].get('moveXEvents',[]))==1 for i in [2,3]),'Clone to named destination lines')
            click(80,20);click(100,317);click(80,20);shot('history');click(80,115)
            save(lambda d:d==original,'Clone exact Undo through History')
            click(460,70);click(475,77)
            click(135,20);shot('playback-rates');click(900,730)
        else:
            click(460,70)
            click(370,250);key('ctrl+a');click(515,H-44);click(245,125);click(210,311)
            wheel(4,18);click(110,247);shot('clone-events');click(110,225);shot('clone-chooser')
            click(120,110);xdo('type','--clearmodifiers','Destination');time.sleep(.3);shot('clone-filtered')
            click(90,199);click(90,236);click(105,310);last_clone_action()
            save(lambda d:all(len(d['judgeLineList'][i]['eventLayers'][0].get('moveXEvents',[]))==1 for i in [2,3]),'Compact Clone action')
            key('ctrl+z');save(lambda d:d==original,'Compact Clone Undo')
            click(475,77);click(30,H-44);drag((220,150),(130,154));shot('horizontal-camera')
            click(78,H-44);click(275,200)
            moved=save(lambda d:any(n['positionX']>675 for n in d['judgeLineList'][2]['notes']),'Horizontal camera authors offscreen notes')
            moved_beat=moved['judgeLineList'][2]['notes'][0]['startTime']
            key('ctrl+z');click(110,56);click(275,200)
            centered=save(lambda d:len(d['judgeLineList'][2]['notes'])==1,'Reset horizontal camera')
            assert abs(centered['judgeLineList'][2]['notes'][0]['positionX'])<=675
            assert centered['judgeLineList'][2]['notes'][0]['startTime']==moved_beat,'Horizontal swipe changed time'
            key('ctrl+z');save(lambda d:d==original,'Camera test exact Undo')
        # All authoring operations keep one reversible history entry.
        (OUT/'result.txt').write_text('PASS\nNative fullscreen note tap/source identity/delete/Undo; named multi-line Clone/Undo; full-size metadata/Storyboard/history/rate forms and compact chooser/scrolling.\n')
        print('Advanced input PASS:',OUT,flush=True)
    except BaseException:
        if window and process.poll() is None:shot('failure')
        raise
    finally:
        process.kill();process.wait(timeout=5)
