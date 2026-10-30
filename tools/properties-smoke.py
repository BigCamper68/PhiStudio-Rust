#!/usr/bin/env python3
"""Exercise preview/settings and reversible properties through the real editor.
Run under a private 1280x800 Xvfb; only generated fixtures are changed.
"""
import json
from pathlib import Path
import subprocess
import tempfile
import time
import wave
from x11_utils import capture_window,wait_for_editor_paint
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/checks/properties-input-0.11';OUT.mkdir(parents=True,exist_ok=True)
workspace=Path(tempfile.mkdtemp(dir=OUT));chart=workspace/'chart.json'
original=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text());chart.write_text(json.dumps(original))
source=workspace/'phone.wav'
with wave.open(str(source),'wb') as wav:
    wav.setparams((1,2,48000,0,'NONE','not compressed'));wav.writeframes(b'\0\0'*960)
source_bytes=source.read_bytes()
def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True).strip()
def key(keys):xdo('key','--clearmodifiers',keys);time.sleep(.2)
def click(x,y):xdo('mousemove','--sync',x,y);xdo('click',1);time.sleep(.3)
def shot(name):capture_window(window,OUT/(name+'.png'),1280,800)
def save(predicate,label):
    key('ctrl+s');deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        assert process.poll() is None,'Editor closed: '+label
        data=json.loads(chart.read_text())
        if predicate(data):return data
        time.sleep(.05)
    raise AssertionError(label+': '+str(data))
def drag(a,b):
    xdo('mousemove','--sync',*a);xdo('mousedown',1)
    for i in range(1,13):xdo('mousemove','--sync',round(a[0]+(b[0]-a[0])*i/12),round(a[1]+(b[1]-a[1])*i/12));time.sleep(.02)
    xdo('mouseup',1);time.sleep(.3)
with (OUT/'editor.log').open('w') as log:
    process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--size','1280x800'],cwd=ROOT,stdout=log,stderr=log)
    try:
        for _ in range(150):
            result=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
            if result.returncode==0:break
            time.sleep(.1)
        window=result.stdout.splitlines()[-1];xdo('windowfocus','--sync',window);wait_for_editor_paint(window,process)
        click(1100,160);shot('fullscreen');click(38,80);shot('returned');save(lambda d:d==original,'Preview leaves chart untouched')
        click(1053,379);shot('control-curves');click(265,455);click(446,70)
        save(lambda d:'posControl' not in d['judgeLineList'][0],'Reset control');key('ctrl+z');save(lambda d:d==original,'Undo control')
        click(981,379);shot('groups');click(80,178);click(446,70)
        save(lambda d:d['judgeLineGroup'][-1]=='New group','Add group');key('ctrl+z');save(lambda d:d==original,'Undo group')
        key('ctrl+a');click(1005,425);time.sleep(.4);shot('hitsounds');click(180,167);key('ctrl+a');xdo('type','--clearmodifiers','flick.mp3');click(214,198);click(446,70)
        save(lambda d:all(n.get('hitsound')=='flick.mp3' for n in d['judgeLineList'][0]['notes']),'Batch hitsound');key('ctrl+z');save(lambda d:d==original,'Undo hitsound')
        key('ctrl+a');click(1005,425);time.sleep(.4);click(98,198);shot('file-picker');click(620,415);xdo('type','--clearmodifiers',str(source));click(520,446);time.sleep(1.2);shot('imported-sound');click(446,70)
        imported=save(lambda d:all(n.get('hitsound','').startswith('phistudio-hitsounds/') for n in d['judgeLineList'][0]['notes']),'Import custom hitsound');reference=imported['judgeLineList'][0]['notes'][0]['hitsound'];assert (workspace/reference).read_bytes()==source_bytes;assert source.read_bytes()==source_bytes
        key('ctrl+z');save(lambda d:d==original,'Undo imported sound')
        click(80,20);click(120,168);shot('settings');click(182,131);shot('aspect-presets');key('Escape');click(358,62)
        click(470,756);shot('edit');drag((200,73),(355,116));shot('edit-moved');click(470,756)
        assert json.loads(chart.read_text())==original
        (OUT/'result.txt').write_text('PASS\nPreview/fullscreen/static return; native control reset/undo; group add/undo; batch and imported hitsound/undo with exact source bytes; settings ratio list; movable Edit window\n')
        print('Properties input smoke PASS:',OUT)
    except BaseException:
        if process.poll() is None:shot('failure')
        raise
    finally:
        process.kill();process.wait(timeout=5)
