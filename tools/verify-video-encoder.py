#!/usr/bin/env python3
"""Run the app's AVFoundation encoder against the actual macOS media framework."""
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[1]
out=ROOT/'artifacts/checks/native-video';out.mkdir(parents=True,exist_ok=True)
command=['xcrun','clang','-fobjc-arc','-fblocks','-Wno-deprecated-declarations',str(ROOT/'ios/native/PhiStudioVideo.m'),str(ROOT/'tools/video-encoder-probe.m'),'-o',str(out/'video-probe')]
for framework in ['Foundation','AVFoundation','CoreMedia','CoreVideo','AudioToolbox']:
    command+=['-framework',framework]
subprocess.run(command,check=True)
result=subprocess.run([out/'video-probe',out],text=True,capture_output=True,timeout=180)
print(result.stdout,flush=True)
print(result.stderr,flush=True)
(out/'probe.log').write_text(result.stdout+result.stderr)
if result.returncode:raise SystemExit(result.returncode)
(out/'result.txt').write_text(result.stdout)
