#!/usr/bin/env python3
"""Install the Linux desktop binary, launcher and the shared IPA artwork."""
import argparse
from pathlib import Path
import shutil
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=ROOT/'target/release/phistudio')
parser.add_argument('--prefix',type=Path,default=Path.home()/'.local')
args=parser.parse_args()
source=args.binary.expanduser().resolve(strict=True)
prefix=args.prefix.expanduser().resolve()
binary=prefix/'bin/phistudio'
icon=prefix/'share/icons/hicolor/512x512/apps/phistudio-rust.png'
launcher=prefix/'share/applications/phistudio-rust.desktop'
for path in [binary,icon,launcher]:path.parent.mkdir(parents=True,exist_ok=True)
if source!=binary:shutil.copy2(source,binary)
binary.chmod(binary.stat().st_mode|0o111)
shutil.copy2(ROOT/'assets/icons/phistudio-512.png',icon)
# Desktop Entry Exec is not a shell command. Quote its reserved characters.
executable=str(binary).replace('\\','\\\\').replace('"','\\"').replace('$','\\$').replace('`','\\`').replace('%','%%')
text=(ROOT/'desktop/phistudio-rust.desktop').read_text().replace('Exec=phistudio',f'Exec="{executable}"')
launcher.write_text(text)
print(f'Installed {binary}, {launcher} and {icon}')
