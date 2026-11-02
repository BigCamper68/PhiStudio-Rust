#!/usr/bin/env python3
"""Real X11 clicks, holds and flicks reach the native prpr Judge."""
from pathlib import Path
import argparse,json,subprocess,time
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,default=ROOT/'artifacts/checks/playtest-real-input');parser.add_argument('--modes',nargs='+');args=parser.parse_args()
OUT=args.output.resolve();OUT.mkdir(parents=True,exist_ok=True)
def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True).strip()
completed=[]
for mode in args.modes or ['miss','play','good','bad','retina','noise-blocked','noise-ready','noise-subtract','noise-latch','noise-enter','noise-retina','noise-next-blocked','noise-next-ready','noise-next-subtract']:
 target=OUT/mode;target.mkdir(exist_ok=True);(target/'ready').unlink(missing_ok=True)
 with (target/'run.log').open('w')as log:
  p=subprocess.Popen([ROOT/'target/debug/examples/playtest-input',target,mode],cwd=ROOT,stdout=log,stderr=log)
  try:
   deadline=time.monotonic()+25
   while not(target/'ready').exists()and p.poll()is None and time.monotonic()<deadline:time.sleep(.01)
   assert(target/'ready').exists(),(target/'run.log').read_text()
   start=time.monotonic()
   window=xdo('search','--onlyvisible','--name','^Playtest input regression$').splitlines()[-1]
   xdo('windowfocus','--sync',window)
   def at(seconds):time.sleep(max(0,start+seconds-time.monotonic()))
   if mode in ['good','bad']:
    xdo('mousemove','--sync',640,360)
    at(.2);xdo('click',1)
   elif mode in ['noise-latch','noise-enter']:
    # Test a latched contact after disappearance or entry during a Hold.
    # Releasing permits the later Drag/Flick to work normally.
    xdo('mousemove','--sync',640,360)
    at(.99);xdo('click',1)
    if mode=='noise-latch':at(1.5);xdo('mousedown',1)
    else:
     at(1.99);xdo('mousedown',1)
     at(2.25);xdo('mousemove','--sync',640,80)
     at(2.65);xdo('mousemove','--sync',640,360)
    at(3.03);xdo('mouseup',1)
    at(3.46);xdo('mousedown',1)
    at(3.58);xdo('mouseup',1)
    at(4.38);xdo('mousemove','--sync',500,360);xdo('mousedown',1)
    for t,x in [(4.41,540),(4.44,590),(4.47,640),(4.50,690)]:at(t);xdo('mousemove','--sync',x,360)
    at(4.56);xdo('mouseup',1)
   elif mode!='miss':
    xdo('mousemove','--sync',640,360)
    at(.99);xdo('click',1)
    at(1.99);xdo('mousedown',1)
    at(3.03);xdo('mouseup',1)
    at(3.46);xdo('mousedown',1)
    at(3.58);xdo('mouseup',1)
    at(4.38);xdo('mousemove','--sync',500,360);xdo('mousedown',1)
    for t,x in [(4.41,540),(4.44,590),(4.47,640),(4.50,690)]:at(t);xdo('mousemove','--sync',x,360)
    at(4.56);xdo('mouseup',1)
   assert p.wait(timeout=15)==0,(target/'run.log').read_text()
   report=json.loads((target/'report.json').read_text())
   print(mode,{'counts':report['counts'],'score':report['score'],'audio_cutoffs':sorted({r['cutoff'] for r in report['noise_audio']})},flush=True)
   if mode=='miss':assert report['counts']==[0,0,0,4]and report['score']==0
   elif mode in ['noise-blocked','noise-retina','noise-next-blocked']:assert report['counts']==[0,0,0,4]
   elif mode=='noise-enter':assert report['counts'][0]+report['counts'][1]==3 and report['counts'][2:]==[0,1]
   elif mode=='noise-latch':assert report['counts'][0]+report['counts'][1]==2 and report['counts'][2:]==[0,2]
   elif mode=='good':assert report['counts']==[0,1,0,3]
   elif mode=='bad':assert report['counts']==[0,0,1,3]
   else:assert report['counts'][0]+report['counts'][1]==4 and report['counts'][2:]==[0,0]
   if mode in ['noise-blocked','noise-retina','noise-latch','noise-enter','noise-next-blocked']:
    assert any(r['cutoff']==1500 for r in report['noise_audio'] if 2.1<r['seconds']<2.6),report['noise_audio']
    assert all(r['cutoff']==22000 for r in report['noise_audio'] if r['seconds']>3.15),report['noise_audio']
   if mode in ['noise-ready','noise-subtract','noise-next-ready','noise-next-subtract']:
    assert all(r['cutoff']==22000 for r in report['noise_audio']),report['noise_audio']
   completed.append(mode)
  finally:
   if p.poll()is None:p.terminate();p.wait(timeout=5)
(OUT/'result.txt').write_text('PASS: real input scenarios: '+', '.join(completed)+'\n')
