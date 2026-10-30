#!/usr/bin/env python3
"""Run under private Xvfb after building --example video-export-probe.
Requires ffmpeg/ffprobe in PATH. Compares the MP4 against actual prpr frame PNGs.
"""
import argparse,json,os,re,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,default=ROOT/'artifacts/checks/video-export')
args=parser.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
frames=out/'frames';frames.mkdir(exist_ok=True)
env=dict(os.environ,PHISTUDIO_TEST_MOVIE_FRAMES=str(frames))
with (out/'probe.log').open('w') as log:
    subprocess.run([ROOT/'target/debug/examples/video-export-probe',out],cwd=ROOT,env=env,stdout=log,stderr=log,check=True,timeout=180)
report=json.loads((out/'result.json').read_text())
for name,count,audio in [('whole-song',113,True),('range-muted',30,False)]:
    source=out/(name+'.mp4')
    meta=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',source],text=True))
    (out/(name+'-streams.json')).write_text(json.dumps(meta,indent=2)+'\n')
    video=[s for s in meta['streams'] if s['codec_type']=='video'];sound=[s for s in meta['streams'] if s['codec_type']=='audio']
    assert len(video)==1 and video[0]['codec_name']=='h264'
    assert [video[0]['width'],video[0]['height']]==[640,360]
    assert video[0]['avg_frame_rate']=='30/1' and int(video[0]['nb_frames'])==count
    assert abs(float(meta['format']['duration'])-count/30)<0.001
    assert len(sound)==int(audio)
    if audio:assert sound[0]['codec_name']=='aac' and sound[0]['sample_rate']=='48000' and sound[0]['channels']==2
    prefix='0.000000' if audio else '1.250000'
    stats=out/(name+'-psnr.txt')
    subprocess.run(['ffmpeg','-nostdin','-hide_banner','-loglevel','error','-framerate','30','-i',str(frames/(prefix+'-%06d.png')),'-i',source,'-filter_complex',f'[0:v]format=yuv420p[ref];[ref][1:v]psnr=stats_file={stats}', '-frames:v',str(count),'-f','null','-'],check=True)
    scores=[float(v) for v in re.findall(r'psnr_avg:([\d.]+)',stats.read_text())]
    assert len(scores)==count and min(scores)>32,(name,min(scores))
    report[name]={'frames':count,'duration':meta['format']['duration'],'minimum_frame_psnr_db':min(scores),'audio':audio}
# Decode AAC, measuring actual PCM rather than merely trusting the stream header.
audio=subprocess.check_output(['ffmpeg','-nostdin','-v','error','-i',out/'whole-song.mp4','-map','0:a:0','-f','s16le','-acodec','pcm_s16le','pipe:1'])
import array,math
samples=array.array('h',audio)
rms=math.sqrt(sum((v/32768.)**2 for v in samples)/len(samples))
assert rms>0.01,rms
report['decoded_audio_rms']=rms
(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
