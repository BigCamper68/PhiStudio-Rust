#!/usr/bin/env python3
"""Capture only this editor's audio in a private PulseAudio sink.

Run preview-reference.py first to generate the native pack's reference WAVs.
Run this script under Xvfb after building phistudio-app. Nothing is recorded
from the default output or other apps. The private sink is removed on exit.
"""
import array
import argparse
import ctypes
import hashlib
import json
import math
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import uuid
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, default=ROOT / "artifacts/checks/playback")
parser.add_argument("--music-volume", type=float, default=1.)
parser.add_argument("--hitsound-volume", type=float, default=1.)
parser.add_argument("--rate", type=float, choices=[0.5,0.75,1.], default=1.)
parser.add_argument("--hitsound-delay-ms", type=float, default=0.)
parser.add_argument("--drag-delay-ms", type=float, default=0.)
parser.add_argument("--skin-directory", type=Path)
parser.add_argument("--pack-reference", type=Path)
args = parser.parse_args()
assert 0 < args.music_volume <= 1 and 0 < args.hitsound_volume <= 1
OUTPUT = args.output.resolve()
REFERENCE = ROOT / "artifacts/checks/preview-reference"
OUTPUT.mkdir(parents=True, exist_ok=True)
assert (REFERENCE / "result.txt").read_text().startswith("PASS")

# A small native loop keeps sample-by-sample waveform matching inexpensive on
# ARM64. Input is the captured output, not a mock backend or a scheduled counter.
correlation_source = r'''
#include <math.h>
#include <stddef.h>
double correlate(const float *record, long n, const float *reference, long m,
                 long start, long end, long *best) {
    double ref_energy = 0, maximum = -1;
    for (long i=0; i<m; i+=8) ref_energy += reference[i]*reference[i];
    if (start<0) start=0;
    if (end>n-m) end=n-m;
    for (long shift=start; shift<end; shift++) {
        double dot=0, energy=0;
        for (long i=0; i<m; i+=8) {
            double a=record[shift+i], b=reference[i];
            dot+=a*b; energy+=a*a;
        }
        double value=dot/sqrt(energy*ref_energy+1e-30);
        if (value>maximum) { maximum=value; *best=shift; }
    }
    return maximum;
}
'''
library = OUTPUT / "correlation.so"
subprocess.run(["cc", "-O3", "-shared", "-fPIC", "-x", "c", "-o", library, "-", "-lm"],
               input=correlation_source, text=True, check=True)
correlate = ctypes.CDLL(str(library)).correlate
pointer = ctypes.POINTER(ctypes.c_float)
correlate.argtypes = [pointer, ctypes.c_long, pointer, ctypes.c_long,
                      ctypes.c_long, ctypes.c_long, ctypes.POINTER(ctypes.c_long)]
correlate.restype = ctypes.c_double


def float_array(values):
    samples = array.array("f", values)
    return (ctypes.c_float * len(samples)).from_buffer(samples)


def reference_clip(kind):
    with wave.open(str((args.pack_reference or REFERENCE) / f"{kind}-reference.wav"), "rb") as audio:
        assert audio.getframerate() == 48000 and audio.getnchannels() == 1
        data = array.array("h", audio.readframes(min(audio.getnframes(), 7200)))
    return float_array(value / 32768 for value in data)


clips = {name: reference_clip(name) for name in ["click", "flick", "drag"]}
sink = "phistudio_preview_" + uuid.uuid4().hex[:12]
module = subprocess.check_output(["pactl", "load-module", "module-null-sink",
                                  f"sink_name={sink}", "rate=48000", "channels=2"], text=True).strip()
reports = []
# Keep this private sink running through long paused frames. Pulse's idle
# suspension otherwise omits silent wall time from the monitor recording.
with open('/dev/zero','rb') as silence:
    keepalive = subprocess.Popen(['pacat','--playback',f'--device={sink}','--raw','--format=s16le','--rate=48000','--channels=2'],stdin=silence,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
try:
    subprocess.run(["pactl", "set-sink-volume", sink, "100%"], check=True)
    subprocess.run(["pactl", "set-sink-mute", sink, "0"], check=True)
    for name, chart_offset, package_offset in [("positive", 100, 0.15), ("negative", -200, -0.15)]:
        directory = OUTPUT / name
        directory.mkdir(exist_ok=True)
        workspace = Path(tempfile.mkdtemp(prefix="workspace-", dir=directory))
        if args.skin_directory:
            skin = workspace / 'skins/skin-probe'
            shutil.copytree(args.skin_directory, skin / 'assets')
            skin.joinpath('.phistudio-skin.json').write_text(json.dumps(dict(id='skin-probe',name='Preview Skin Fixture',author='PhiStudio Tests',description='',assets='assets')))
        (workspace / "editor-settings.json").write_text(json.dumps({"version": 1, "music_volume": args.music_volume, "hitsound_volume": args.hitsound_volume, "options":{"hitsound_delay_ms":args.hitsound_delay_ms,"drag_delay_ms":args.drag_delay_ms,"highlight_current_line":False,"mark_active_line_id":False}}))
        if args.skin_directory:
            path=workspace / 'editor-settings.json'
            settings=json.loads(path.read_text());settings['options']['skin']='skin-probe'
            path.write_text(json.dumps(settings))
        chart = json.loads((REFERENCE / "chart.json").read_text())
        chart["META"]["offset"] = chart_offset
        music_file = directory / "tone.wav"
        with wave.open(str(music_file), "wb") as music:
            music.setparams((1, 2, 48000, 0, "NONE", "not compressed"))
            tone = array.array("h", (round(0.025*32767*math.sin(2*math.pi*220*i/48000)) for i in range(48000)))
            music.writeframes(tone.tobytes()*8)
        archive = directory / "probe.pez"
        with zipfile.ZipFile(archive, "w") as package:
            package.writestr("chart.json", json.dumps(chart))
            package.write(REFERENCE / "cover.png", "cover.png")
            package.write(music_file, "song.wav")
            package.writestr("info.yml", f"chart: chart.json\nmusic: song.wav\nillustration: cover.png\noffset: {package_offset}\n")
        before = hashlib.sha256(archive.read_bytes()).hexdigest()
        raw = directory / "captured.pcm"
        with raw.open("wb") as captured, (directory / "recorder.log").open("w") as recorder_log:
            recording_start = time.time()
            recorder = subprocess.Popen(["parec", f"--device={sink}.monitor", "--raw", "--format=s16le",
                                         "--rate=48000", "--channels=2", "--latency-msec=10"],
                                        stdout=captured, stderr=recorder_log)
            try:
                environment = dict(os.environ, PULSE_SINK=sink, PHISTUDIO_PROBE_RATE=str(args.rate))
                with (directory / "editor.log").open("w") as log:
                    subprocess.run([ROOT / "target/debug/phistudio", "--workspace", workspace,
                                    "--package", archive, "--playback-smoke", directory],
                                   cwd=ROOT, env=environment, stdout=log, stderr=log, check=True, timeout=55)
            finally:
                recorder.terminate()
                recorder.wait(timeout=5)
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == before
        assert (directory / "result.txt").read_text().startswith("PASS")
        stereo = array.array("h", raw.read_bytes())
        with wave.open(str(directory / "captured.wav"), "wb") as wav:
            wav.setparams((2,2,48000,0,"NONE","not compressed"))
            wav.writeframes(stereo.tobytes())
        mono = float_array((left+right)/65536 for left,right in zip(stereo[::2],stereo[1::2]))
        events = json.loads((directory / "events.json").read_text())
        by_name = {event["event"]: event for event in events}
        start = by_name["start"]["unix_seconds"]
        offset = chart_offset/1000 + package_offset
        # A suspended null sink does not necessarily start streaming when the
        # recorder process starts. Align the capture using the first Tap's
        # actual waveform and the original Judge's timestamp, then verify the
        # other cues independently at their relative judgement times.
        first_transient = next(i for i,value in enumerate(mono) if abs(value)>0.04)
        anchor = ctypes.c_long()
        quality = correlate(mono,len(mono),clips["click"],len(clips["click"]),
                            first_transient-2400,first_transient+2400,ctypes.byref(anchor))
        assert quality>0.7, (name,"No initial pack Tap",quality)
        judgements = by_name["play-6"]["playback"]["judgement_events"]
        judged_hits = [min((hit[0] for hit in judgements), key=lambda t:abs(t-expected))
                       for expected in [0.5,1.,1.5,2.,3.5]]
        delayed = args.hitsound_delay_ms != 0 or args.drag_delay_ms != 0
        actual_hits = [t+(args.drag_delay_ms if kind=="drag" else args.hitsound_delay_ms)/1000 for t,kind in [(0.5,"click"),(1.,"flick"),(1.5,"drag"),(2.,"click"),(3.5,"click")]] if delayed else judged_hits
        point = by_name["play-1"]
        chart_clock = point["playback"]["transport"]["audio_seconds"]-offset
        first_hit_epoch = point["unix_seconds"]-(chart_clock-actual_hits[0])/args.rate
        recording_start = first_hit_epoch-anchor.value/48000
        matches = []
        for (chart_time, kind), actual_time in zip([(0.5,"click"),(1.,"flick"),(1.5,"drag"),(2.,"click"),(3.5,"click")],actual_hits):
            predicted = anchor.value+round((actual_time-actual_hits[0])*48000/args.rate)
            best = ctypes.c_long()
            quality = correlate(mono,len(mono),clips[kind],len(clips[kind]),
                                predicted-7200,predicted+7200,ctypes.byref(best))
            matches.append({"chart_seconds":chart_time,"kind":kind,"correlation":quality,
                            "shift_seconds":(best.value-predicted)/48000})
            assert abs((best.value-predicted)/48000) < 0.08, (name,kind,"Hitsound timing differs",matches[-1])
            assert quality > 0.7, f"{name}: captured {kind} does not match original pack: {matches[-1]}"
        def rms_between(a,b):
            lo = max(0,round((a-recording_start)*48000))
            hi = min(len(mono),round((b-recording_start)*48000))
            assert hi>lo+1000, (a,b,lo,hi)
            return math.sqrt(sum(mono[i]*mono[i] for i in range(lo,hi))/(hi-lo))
        paused = rms_between(by_name["paused"]["unix_seconds"]+0.1,by_name["pause-end"]["unix_seconds"]-0.05)
        seeking = rms_between(by_name["seek-paused"]["unix_seconds"]+0.1,by_name["resume"]["unix_seconds"]-0.05)
        resumed = rms_between(by_name["resume"]["unix_seconds"]+0.1,by_name["fake-passed"]["unix_seconds"]-0.05)
        forward = rms_between(by_name["fake-passed"]["unix_seconds"]+0.1,by_name["forward-seek-end"]["unix_seconds"]-0.05)
        scrubbed = rms_between(by_name["scrub-held"]["unix_seconds"]+0.1,by_name["scrub-position"]["unix_seconds"]-0.05)
        released = rms_between(by_name["scrub-resumed"]["unix_seconds"]+0.1,by_name["scrub-resumed-end"]["unix_seconds"]-0.04)
        assert paused < 0.0001 and seeking < 0.0001, (name,paused,seeking)
        assert 0.012 < resumed / args.music_volume < 0.025 and 0.012 < forward / args.music_volume < 0.025, (name,resumed,forward)
        assert scrubbed<0.0001 and 0.012<released / args.music_volume<0.025, (name,scrubbed,released)
        for event in events:
            playback = event["playback"]
            assert playback["transport"]["rate"] == args.rate
            assert playback["sound_delays_ms"] == [args.hitsound_delay_ms,args.drag_delay_ms]
            assert abs(playback["offset_seconds"]-offset)<1e-12
            if event["event"].startswith("play-") and playback["transport"]["audio_seconds"]>0.2:
                difference = playback["transport"]["audio_seconds"]-playback["transport"]["music"]["position"]
                assert abs(difference)<0.15, (event["event"],difference)
        report = {"case":name,"offset_seconds":offset,"input_unchanged":True,"hitsounds":matches,
                  "paused_rms":paused,"seek_paused_rms":seeking,"resumed_rms":resumed,"forward_seek_rms":forward,
                  "finger_scrubbing_rms":scrubbed,"finger_release_rms":released, "music_volume":args.music_volume, "hitsound_volume":args.hitsound_volume, "rate":args.rate,"sound_delays_ms":[args.hitsound_delay_ms,args.drag_delay_ms]}
        reports.append(report)
        (OUTPUT / "report.json").write_text(json.dumps(reports,indent=2)+"\n")
        print("PASS:",name,"offset",offset,"waveform correlations",[round(m["correlation"],3) for m in matches],flush=True)
finally:
    keepalive.terminate();keepalive.wait(timeout=5)
    subprocess.run(["pactl","unload-module",module],check=True)
(OUTPUT / "result.txt").write_text("PASS\nReal CPAL/Pulse output captured only from an isolated test sink\nPositive/negative chart + package offsets; original pack waveforms matched\nPause and paused seeks are silent; resume and forward seeks do not replay hits\n")
