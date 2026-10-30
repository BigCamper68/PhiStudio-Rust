#!/usr/bin/env python3
"""Actual input for Storyboard properties, color palette, themes and skin library.
Run on a private Xvfb. Only generated fixture projects and copied test packs change.
"""
import argparse,copy,hashlib,json,shutil,struct,subprocess,tempfile,time,zipfile,zlib
from pathlib import Path
from x11_utils import capture_window,wait_for_editor_paint
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--stage',default='all');p.add_argument('--compact',action='store_true');args=p.parse_args()
W,H=(873,393)if args.compact else(1280,800)
OUT=ROOT/('artifacts/checks/palette-skins-compact'if args.compact else'artifacts/checks/palette-skins-input');OUT.mkdir(parents=True,exist_ok=True)
workspace=Path(tempfile.mkdtemp(prefix='phistudio-skin-ui-'));package=workspace/'fixture.pez'
raw=json.loads((ROOT/'assets/charts/phistudio-demo.rpe.json').read_text());raw['META'].update(name='Palette and skins',song='',background='',offset=0);raw['BPMList']=[dict(bpm=120.,startTime=[0,0,1])]
line=raw['judgeLineList'][0];raw['judgeLineList']=[line];template=copy.deepcopy(line['notes'][0]);line['notes']=[]
for kind,beat,end,x in [(1,1,1,-200),(2,1,3,200),(4,2,2,0)]:
 note=copy.deepcopy(template);note.update(type=kind,startTime=[beat,0,1],endTime=[end,0,1],positionX=x);line['notes'].append(note)
line['numOfNotes']=3
line['extended']={'colorEvents':[dict(startTime=[0,0,1],endTime=[2,0,1],start=[255,255,255],end=[100,150,255],easingType=6)],'scaleXEvents':[dict(startTime=[0,0,1],endTime=[2,0,1],start=1.,end=2.,easingType=9)]}
with zipfile.ZipFile(package,'w')as z:z.writestr('chart.json',json.dumps(raw));z.writestr('info.yml','chart: chart.json\n')
skin=workspace/'skin.zip';shutil.copyfile(ROOT/'artifacts/checks/skin-reference/Preview Skin Fixture.zip',skin)
def xdo(*a):return subprocess.check_output(['xdotool',*map(str,a)],text=True).strip()
def click(x,y):xdo('mousemove','--sync',round(x),round(y));xdo('click',1);time.sleep(.25)
def key(k):xdo('key','--clearmodifiers',k);time.sleep(.15)
def wheel(b,n=12,x=750,y=230):xdo('mousemove','--sync',x,y);xdo('click','--repeat',n,'--delay',15,b);time.sleep(.3)
def shot(n):capture_window(window,OUT/(n+'.png'),W,H)
def pixel(name,x,y):
 data=(OUT/(name+'.png')).read_bytes();at=8;parts=[]
 while at<len(data):
  n=struct.unpack('>I',data[at:at+4])[0]
  if data[at+4:at+8]==b'IDAT':parts.append(data[at+8:at+8+n])
  at+=n+12
 rows=zlib.decompress(b''.join(parts));at=y*(W*3+1)+1+x*3
 return tuple(rows[at:at+3])
def green_pixels(name):
 data=(OUT/(name+'.png')).read_bytes();at=8;parts=[]
 while at<len(data):
  size=struct.unpack('>I',data[at:at+4])[0]
  if data[at+4:at+8]==b'IDAT':parts.append(data[at+8:at+8+size])
  at+=size+12
 rows=zlib.decompress(b''.join(parts));count=0
 for y in range(100,H-80):
  for x in range(40,round(W*.42)):
   at=y*(W*3+1)+1+x*3
   count+=rows[at:at+3]==bytes((35,240,85))
 return count
def save():key('ctrl+s');time.sleep(.6);return json.loads(chart.read_text())
with (OUT/'editor.log').open('w')as log:
 process=subprocess.Popen([ROOT/'target/debug/phistudio','--workspace',workspace,'--package',package,'--size',f'{W}x{H}'],cwd=ROOT,stdout=log,stderr=log)
 try:
  window=None;deadline=time.monotonic()+25
  while time.monotonic()<deadline and process.poll()is None:
   r=subprocess.run(['xdotool','search','--onlyvisible','--name','^PhiStudio Rust$'],capture_output=True,text=True)
   if r.returncode==0:window=r.stdout.splitlines()[-1];break
   time.sleep(.1)
  assert window;xdo('windowfocus','--sync',window);wait_for_editor_paint(window,process,height=H);time.sleep(.5)
  chart=next(workspace.glob('projects/*/working/*'));original=json.loads(chart.read_text());shot('editor')
  if args.stage=='editor':raise SystemExit(0)
  click(400 if args.compact else 580,62);click(400 if args.compact else 580,133);shot('storyboard')
  click(462 if args.compact else 700,H-209);shot('story-properties')
  if args.stage=='story':raise SystemExit(0)
  click(760 if args.compact else 1110,232 if args.compact else 306);shot('palette')
  if args.stage=='palette':raise SystemExit(0)
  click(490,150);shot('palette-picked')
  sample=pixel('palette',490,150);swatch=pixel('palette-picked',350,286 if args.compact else 423)
  assert max(abs(a-b)for a,b in zip(sample,swatch))<=3,('Palette visual color differs from selected RGB',sample,swatch)
  click(100,min(H-29,501));wheel(5,18,x=W-110,y=min(230,H-130));shot('story-apply-ready')
  if args.stage=='color':raise SystemExit(0)
  click(W-170,H-125)
  modified=save();assert modified['judgeLineList'][0]['extended']['colorEvents'][0]['start']!=[255,255,255], 'Palette color was not applied'
  key('ctrl+z');assert save()==original,'Color event exact Undo'
  key('ctrl+shift+z');assert save()==modified,'Color event Redo'
  click(368,H-44)
  a=(365 if args.compact else 563,H-185);b=(449 if args.compact else 700,H-185)
  xdo('mousemove','--sync',*a);xdo('mousedown',1)
  for i in range(1,14):xdo('mousemove','--sync',round(a[0]+(b[0]-a[0])*i/13),a[1]);time.sleep(.02)
  xdo('mouseup',1);time.sleep(.3)
  erased=save();assert not erased['judgeLineList'][0].get('extended',{}).get('scaleXEvents') and not erased['judgeLineList'][0].get('extended',{}).get('colorEvents'),'Storyboard eraser missed tracks'
  shot('story-erased');key('ctrl+z');assert save()==modified,'Eraser stroke was not one Undo'
  click(80,20);shot('windows-menu');click(120,168);shot('settings')
  if args.stage=='settings':raise SystemExit(0)
  wheel(5,6,x=250,y=min(230,H-130));shot('skin-settings')
  if args.stage=='skins':raise SystemExit(0)
  source_hash=hashlib.sha256(skin.read_bytes()).hexdigest()
  click(110,345 if args.compact else 360);shot('skin-picker')
  click(W/2,H/2+15);xdo('type','--clearmodifiers',str(skin));click(W/2-137,H/2+46)
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   entries=list(workspace.glob('skins/skin-*/.phistudio-skin.json'))
   if entries:break
   assert process.poll()is None;time.sleep(.1)
  assert len(entries)==1,'Skin import did not publish a valid pack'
  entry=json.loads(entries[0].read_text());assert entry['name']=='Preview Skin Fixture'
  with zipfile.ZipFile(skin)as archive:
   assets=entries[0].parent/entry['assets']
   for name in archive.namelist():
    if not name.endswith('/'):assert (assets/Path(name).relative_to('Fixture')).read_bytes()==archive.read(name),'Imported skin resource bytes changed'

  time.sleep(1.);shot('skin-imported')
  click(180,291 if args.compact else 306);shot('skin-choice')
  if args.stage=='import':raise SystemExit(0)
  key('Escape');click(358,62 if args.compact else 78);shot('custom-skin-grid');assert green_pixels('custom-skin-grid')>100,'Skin did not reach highlighted grid notes'
  click(80,20);click(120,168);click(180,291 if args.compact else 306);click(160,215 if args.compact else 344)
  time.sleep(1.);click(358,62 if args.compact else 78);shot('default-skin-grid');assert green_pixels('default-skin-grid')==0,'Default skin did not restore grid textures'
  click(80,20);click(120,168);click(180,291 if args.compact else 306);click(160,252 if args.compact else 381)
  time.sleep(.8);wheel(5,2,x=250,y=min(230,H-130));shot('skin-delete-ready');click(230,297 if args.compact else 312);wheel(5,3,x=250,y=min(230,H-130));shot('skin-delete-confirm')
  if args.stage=='delete':raise SystemExit(0)
  click(94,234 if args.compact else 249)
  deadline=time.monotonic()+12
  while time.monotonic()<deadline and list(workspace.glob('skins/skin-*/.phistudio-skin.json')):time.sleep(.1)
  assert not list(workspace.glob('skins/skin-*/.phistudio-skin.json')),'Skin deletion did not remove the selected imported pack'
  time.sleep(.8);shot('theme-settings')
  if args.stage=='theme-settings':raise SystemExit(0)
  click(125,254 if args.compact else 269);shot('theme-palette')
  click(540,130);click(560,259 if args.compact else 396);shot('theme-preview');click(100,min(H-29,501))
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   pref=workspace/'editor-settings.json'
   settings=json.loads(pref.read_text())if pref.exists()else{}
   theme=settings.get('options',{}).get('theme',{}).get('colors',[])
   if theme and theme[6]!=[115,237,202,255]:break
   time.sleep(.1)
  assert theme and theme[6]!=[115,237,202,255],'Theme choice did not persist'
  assert settings['options'].get('skin')is None,'Deleting the active skin did not restore Default'
  assert json.loads(chart.read_text())==modified,'Theme/skin preferences changed chart data'
  shot('theme-applied');click(358,62 if args.compact else 78);shot('themed-editor')
  assert hashlib.sha256(skin.read_bytes()).hexdigest()==source_hash
  (OUT/'result.txt').write_text('PASS\nStoryboard native easing properties; color palette apply/Undo; one-stroke Erase/Undo; skin ZIP picker/import/exact bytes/highlighted grid/default switching/deletion; theme palette and persistence with unchanged chart.\n')
  print('Palette and skin input PASS:',OUT,flush=True)
  (OUT/'workspace.txt').write_text(str(workspace)+'\n')
 except BaseException:
  if window and process.poll()is None:shot('last')
  raise
 finally:process.kill();process.wait(timeout=5)
