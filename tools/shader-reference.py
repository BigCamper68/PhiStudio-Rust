#!/usr/bin/env python3
"""Compare original GameScene shader passes with editor output; generated fixtures only."""
import copy,json,shutil,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];BASE=ROOT/'artifacts/checks/preview-reference';OUT=ROOT/'artifacts/checks/shader-reference'
OUT.mkdir(parents=True,exist_ok=True)
for item in BASE.iterdir():
    if item.name in ['pack','rank','mod']:
        shutil.copytree(item,OUT/item.name,dirs_exist_ok=True)
    elif item.name in ['chart.json','chart-attached.json','cover.png','wide-cover.png','song.wav','back.png','retry.png','resume.png','proceed.png','player.jpg']:
        shutil.copy2(item,OUT/item.name)
cases=[]
for shader in ['chromatic','circleBlur','fisheye','glitch','grayscale','noise','pixel','radialBlur','shockwave','vignette']:
    for global_ in [False,True]:
        cases.append(dict(label=f'{shader}-{global_}',aspect=4/3 if global_ else 16/9,extra=dict(bpm=120,effects=[dict(start=[1,0,1],end=[7,0,1],shader=shader,global_=global_)])))
        effect=cases[-1]['extra']['effects'][0];effect['global']=effect.pop('global_')
(OUT/'custom.glsl').write_text('''#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D screenTexture;
uniform float time;
uniform vec2 screenSize;
uniform float amount; // %0.5%
uniform vec2 center; // %0.5,0.5%
uniform vec4 tint; // %1.0,0.9,0.8,1.0%
void main(){vec2 p=uv+(center-vec2(0.5))*0.05;vec4 c=texture2D(screenTexture,p);gl_FragColor=vec4(mix(c.rgb,c.bgr,amount)*tint.rgb*(0.9+0.1*sin(time)),1.0);}
''')
kf=lambda a,b:dict(startTime=[0,0,1],endTime=[8,0,1],start=a,end=b,easingType=6,easingLeft=.15,easingRight=.9)
for global_ in [False,True]:
    cases.append(dict(label=f'animated-custom-{global_}',aspect=1.5,extra=dict(bpm=[dict(time=[0,0,1],bpm=120),dict(time=[3,0,1],bpm=180)],effects=[dict(start=[0,0,1],end=[10,0,1],shader='/custom.glsl',global_=global_,vars=dict(amount=[kf(.1,.9)],center=[kf([.3,.4],[.7,.6])],tint=[kf([255,128,64,255],[64,128,255,255])]))])))
    e=cases[-1]['extra']['effects'][0];e['global']=e.pop('global_')
cases.append(dict(label='stacked-local-global',aspect=2.,extra=dict(bpm=120,effects=[dict(start=[1,0,1],end=[7,0,1],shader='pixel',vars=dict(size=9.)),dict(start=[2,0,1],end=[8,0,1],shader='chromatic'),dict(start=[0,0,1],end=[8,0,1],shader='vignette',**{'global':True})])))
(OUT/'shader-cases.json').write_text(json.dumps(cases));subprocess.run([ROOT/'target/debug/examples/preview-reference',OUT],cwd=ROOT,check=True,timeout=240)
print('PASS: original Phira local/global shaders, custom GLSL, animated uniforms, timing and BPM changes',flush=True)
