#!/usr/bin/env python3
"""Exercise 1.18 Tools through real X11 input on phone/iPad viewports.
Run under private Xvfb >=1280x900 after building phistudio-app. Fixtures only.
"""
import argparse,hashlib,json,struct,subprocess,tempfile,time,zlib
from pathlib import Path
from x11_utils import capture_window,wait_for_editor_paint
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,default=ROOT/'artifacts/checks/1.18.0/authoring-input');args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
def xdo(*a):return subprocess.check_output(['xdotool',*map(str,a)],text=True).strip()
for label,(width,height) in [('phone',(873,393)),('ipad',(1194,834))]:
 case=Path(tempfile.mkdtemp(prefix=label+'-',dir=out));store=case/'store';chart=case/'source.rpe.json'
 original=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text());original['META'].update(name='Authoring 1.18 input',song='',background='');original['futureRoot']={'keep':[3,1,None]};line=original['judgeLineList'][0];original['judgeLineList']=[line];line['notes']=[dict(type=1,startTime=[i,0,1],endTime=[i,0,1],positionX=x,above=1,isFake=0,alpha=255,size=1.,speed=1.,visibleTime=999999.,yOffset=0.,futureNote={'id':i})for i,x in enumerate([-270.,0.,270.],1)];line['numOfNotes']=3
 chart.write_text(json.dumps(original));digest=hashlib.sha256(chart.read_bytes()).hexdigest();page=0
 log=(case/'editor.log').open('w');process=subprocess.Popen([ROOT/'target/debug/phistudio','--chart',chart,'--workspace',store,'--size',f'{width}x{height}'],cwd=ROOT,stdout=log,stderr=log)
 try:
  window=None;end=time.monotonic()+25
  while time.monotonic()<end and process.poll() is None:
   r=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
   if r.returncode==0:window=r.stdout.splitlines()[-1];break
   time.sleep(.05)
  assert window;wait_for_editor_paint(window,process,height=height);xdo('windowfocus','--sync',window)
  def click(x,y):xdo('mousemove','--sync',round(x),round(y));xdo('click',1);time.sleep(.25)
  def key(k):xdo('key','--clearmodifiers',k);time.sleep(.2)
  def type_text(s):xdo('type','--clearmodifiers',s);time.sleep(.15)
  def wheel(button,n=35):
   xdo('mousemove','--sync',350,190 if page in (7,8)else 270);xdo('click','--repeat',n,'--delay',12,button);time.sleep(.25)
  def shot(name):
   xdo('mousemove','--sync',520,48);time.sleep(.1);path=case/(name+'.png');capture_window(window,path,width,height);return path
  def live():
   last=store/'last-project.json';path=Path(json.loads(last.read_text())['chart'])if last.exists()else chart;return json.loads(path.read_text())
  def until(test,label):
   end=time.monotonic()+12
   while time.monotonic()<end:
    assert process.poll()is None,'Editor exited: '+label
    try:
     if test():return
    except (FileNotFoundError,json.JSONDecodeError,KeyError):pass
    time.sleep(.05)
   raise AssertionError('Timed out: '+label)
  def save(test,label):key('ctrl+s');until(lambda:(store/'last-project.json').exists()and test(live()),label)
  def choose(index):
   global page
   click(245,125);xdo('mousemove','--sync',215,255);xdo('click','--repeat',15,'--delay',12,4 if index<=4 else 5);time.sleep(.2);click(215,163+37*index if index<=4 else 332-37*(9-index));page=index;wheel(4)
  def buttons():
   path=shot('action-position');png=path.read_bytes();at=8;parts=[]
   while at<len(png):
    n=struct.unpack('>I',png[at:at+4])[0];kind=png[at+4:at+8];data=png[at+8:at+8+n];at+=12+n
    if kind==b'IDAT':parts.append(data)
   raw=zlib.decompress(b''.join(parts));stride=width*3+1;runs=[];start=None
   for y in range(202,height-5):
    rgb=raw[y*stride+1+62*3:y*stride+1+62*3+3]
    if rgb==bytes((29,39,57)):
     if start is None:start=y
    elif start is not None:
     if y-start>=22:runs.append((start+y)//2)
     start=None
   if start is not None and height-5-start>=22:runs.append((start+height-5)//2)
   assert runs,'No reachable action button';return runs
  # Shared window and motion recipe: free layer, preview, save and exact Undo.
  click(515,height-44);choose(5);shot('trajectory-top');wheel(5,65);shot('trajectory-preview');click(110,buttons()[-1]);save(lambda d:len(d['judgeLineList'][0].get('phiStudioTrajectories',[]))==1,'trajectory creation');working=live();assert working['futureRoot']==original['futureRoot'];assert working['judgeLineList'][0]['eventLayers'][0]==original['judgeLineList'][0]['eventLayers'][0];shot('trajectory-created');key('ctrl+z');save(lambda d:'phiStudioTrajectories'not in d['judgeLineList'][0],'trajectory undo')
  # Font outlines and native-parent output; one Undo removes controller/segments.
  choose(6);shot('text-top');wheel(5,65);click(110,buttons()[-1]);time.sleep(.5);wheel(5,20);shot('text-preview');click(110,buttons()[-1]);save(lambda d:len(d['judgeLineList'])>2,'text insertion');d=live();assert d['judgeLineList'][1]['phiStudioTextComposition']['text']=='PhiStudio';assert all(v['father']==1 and v['rotateWithFather']for v in d['judgeLineList'][2:]);shot('text-inserted');key('ctrl+z');save(lambda d:len(d['judgeLineList'])==1,'text undo')
  # Portable library: actual copy, selection, rename, pin and activation.
  choose(0);click(90,227);key('ctrl+c');until(lambda:len(json.loads((store/'clipboard-history/history.json').read_text())['entries'])==1,'clipboard capture');choose(8);shot('clipboard-list');click(88,222);wheel(5,25);row=buttons()[-1];click(130,row-37);type_text('Pinned probe');click(265,row-37);click(202,row);until(lambda:json.loads((store/'clipboard-history/history.json').read_text())['entries'][0]['pinned'],'clipboard pin');click(110,row);shot('clipboard-active');entry=json.loads((store/'clipboard-history/history.json').read_text())['entries'][0];assert entry['name']=='Pinned probe';assert len(live()['judgeLineList'])==1
  # Named configuration is data for forms; selecting it must not paste/edit.
  choose(7);click(130,222);type_text('Saved probe');click(270,222);until(lambda:len(json.loads((store/'tool-configurations.json').read_text())['presets'])==1,'configuration save');before=live();click(100,260);click(120,298);time.sleep(.3);assert live()==before;shot('configuration-loaded')
  assert hashlib.sha256(chart.read_bytes()).hexdigest()==digest,'External source changed'
  (case/'result.txt').write_text('PASS: actual motion recipe create/Undo, text parent insertion/Undo, clipboard name/pin/activation, named configuration save/load; external source unchanged.\n');print(label,'PASS',case,flush=True)
 except BaseException:
  if window and process.poll()is None:shot('failure')
  raise
 finally:process.kill();process.wait(timeout=5);log.close()
(out/'result.txt').write_text('PASS\nPhone and iPad real authoring input workflows.\n')
