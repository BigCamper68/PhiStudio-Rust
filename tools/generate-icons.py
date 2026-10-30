#!/usr/bin/env python3
"""Generate/check application artwork shared with the original IPA icon."""
import argparse
import hashlib
from pathlib import Path
import struct
from icon_art import icon_png, icon_rgba
ROOT=Path(__file__).resolve().parents[1]
check=argparse.ArgumentParser()
check.add_argument('--check',action='store_true')
args=check.parse_args()
assert hashlib.sha256(icon_png(512)).hexdigest()=='2a5357b6f3b288243f9c47cf8fe03d74d08cec4fa5e59dff18c0e3c42ec30762'
files={}
for size in [16,32,48,64,128,256,512,1024]:
    files[f'assets/icons/phistudio-{size}.png']=icon_png(size)
    if size in [16,32,64]:files[f'assets/icons/phistudio-{size}.rgba']=icon_rgba(size)
sizes=[16,32,48,64,128,256]
images=[icon_png(n) for n in sizes]
offset=6+16*len(sizes)
entries=[]
for n,data in zip(sizes,images):
    entries.append(struct.pack('<BBBBHHII',n%256,n%256,0,0,1,24,len(data),offset))
    offset+=len(data)
files['assets/icons/phistudio.ico']=struct.pack('<HHH',0,1,len(sizes))+b''.join(entries)+b''.join(images)
parts=[]
for n,kind in [(16,b'icp4'),(32,b'icp5'),(64,b'icp6'),(128,b'ic07'),(256,b'ic08'),(512,b'ic09'),(1024,b'ic10')]:
    data=icon_png(n)
    parts.append(kind+struct.pack('>I',len(data)+8)+data)
body=b''.join(parts)
files['assets/icons/phistudio.icns']=b'icns'+struct.pack('>I',len(body)+8)+body
for dpi,size in [('mdpi',48),('hdpi',72),('xhdpi',96),('xxhdpi',144),('xxxhdpi',192)]:
    files[f'android/app/src/main/res/mipmap-{dpi}/ic_launcher.png']=icon_png(size)
files['android/app/src/main/res/drawable-nodpi/phistudio_art.png']=icon_png(512)
for name,data in files.items():
    path=ROOT/name
    if args.check:assert path.read_bytes()==data,f'Stale application icon: {path}'
    else:
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_bytes(data)
print(f'PASS: {len(files)} platform icon assets match the original IPA artwork')
