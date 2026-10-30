#!/usr/bin/env python3
"""Build a real arm64 iOS IPA on macOS; signing is deliberately left to its owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import zipfile

from icon_art import icon_png

ROOT = Path(__file__).resolve().parents[1]


def run(*args, capture=False, env=None):
    print("+ " + " ".join(map(str, args)), flush=True)
    result = subprocess.run(list(map(str, args)), cwd=ROOT, check=True, text=True,
                            capture_output=capture, env=env)
    return result.stdout.strip() if capture else None



def compile_assets(staging, app, minimum):
    assets = staging / "Assets.xcassets"
    icons = assets / "AppIcon.appiconset"
    icons.mkdir(parents=True)
    images = []
    for idiom, sizes in [("iphone", [(20, [2, 3]), (29, [2, 3]), (40, [2, 3]), (60, [2, 3])]),
                         ("ipad", [(20, [1, 2]), (29, [1, 2]), (40, [1, 2]), (76, [1, 2]), (83.5, [2])]),
                         ("ios-marketing", [(1024, [1])])]:
        for points, scales in sizes:
            for scale in scales:
                name = f"icon-{idiom}-{points}-{scale}.png"
                (icons / name).write_bytes(icon_png(round(points * scale)))
                images.append({"idiom": idiom, "size": f"{points}x{points}", "scale": f"{scale}x", "filename": name})
    (icons / "Contents.json").write_text(json.dumps({"images": images, "info": {"author": "phistudio", "version": 1}}))
    partial = staging / "asset-info.plist"
    run("xcrun", "actool", assets, "--compile", app, "--platform", "iphoneos",
        "--minimum-deployment-target", minimum, "--app-icon", "AppIcon", "--output-partial-info-plist", partial,
        "--target-device", "iphone", "--target-device", "ipad")
    return plistlib.loads(partial.read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/releases/ios/current")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("An Apple macOS host with Xcode and the iPhoneOS SDK is required")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    info = plistlib.loads((ROOT / "ios/Info.plist").read_bytes())
    minimum = info["MinimumOSVersion"]
    sdk = run("xcrun", "--sdk", "iphoneos", "--show-sdk-path", capture=True)
    sdk_version = run("xcrun", "--sdk", "iphoneos", "--show-sdk-version", capture=True)
    run("rustup", "target", "add", "aarch64-apple-ios")
    env = os.environ | {"IPHONEOS_DEPLOYMENT_TARGET": minimum, "SDKROOT": sdk, "CARGO_BUILD_JOBS": "2"}
    run("cargo", "build", "--locked", "--release", "--target", "aarch64-apple-ios",
        "-p", "phistudio-app", "--bin", "phistudio", env=env)
    staging = output / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    app = staging / "Payload/PhiStudio.app"
    app.mkdir(parents=True)
    binary = app / "PhiStudio"
    shutil.copy2(ROOT / "target/aarch64-apple-ios/release/phistudio", binary)
    binary.chmod(0o755)
    info.update(compile_assets(staging, app, minimum))
    info.update(DTPlatformName="iphoneos", DTPlatformVersion=sdk_version, DTSDKName=f"iphoneos{sdk_version}")
    (app / "Info.plist").write_bytes(plistlib.dumps(info, fmt=plistlib.FMT_BINARY))
    (app / "PkgInfo").write_bytes(b"APPL????")
    run("xcrun", "ibtool", "--compile", app / "LaunchScreen.storyboardc",
        ROOT / "ios/LaunchScreen.storyboard", "--minimum-deployment-target", minimum,
        "--target-device", "iphone", "--target-device", "ipad")
    for icon in (ROOT / "assets/runtime").glob("*.png"):
        shutil.copy2(icon, app / icon.name)
    for name in ["LICENSE", "THIRD_PARTY.md"]:
        shutil.copy2(ROOT / name, app / name)
    shutil.copytree(ROOT / "licenses", app / "licenses")
    (app / "fonts").mkdir()
    shutil.copy2(ROOT / "assets/fonts/SOURCE.json", app / "fonts/SOURCE.json")
    # Rust/ld may insert an ad-hoc signature. The recipient signs the complete
    # app with their own paid certificate and matching provisioning profile.
    signature = subprocess.run(["codesign", "--display", str(binary)], capture_output=True)
    if signature.returncode == 0:
        run("codesign", "--remove-signature", binary)
    arch = run("lipo", "-archs", binary, capture=True)
    if arch != "arm64":
        raise RuntimeError(f"Expected an arm64 device executable, got {arch}")
    load_commands = run("otool", "-l", binary, capture=True)
    dylibs = run("otool", "-L", binary, capture=True)
    if "platform 2" not in load_commands and "platform IOS" not in load_commands:
        raise RuntimeError("Executable does not target physical iOS devices")
    for line in dylibs.splitlines()[1:]:
        dep = line.strip().split(" (", 1)[0]
        if not dep.startswith(("/System/Library/", "/usr/lib/")):
            raise RuntimeError(f"Unbundled dynamic dependency: {dep}")
    name = f"PhiStudio-Rust-{info['CFBundleShortVersionString']}-ios-arm64.ipa"
    ipa = output / name
    with zipfile.ZipFile(ipa, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for path in sorted((staging / "Payload").rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(staging))
    with zipfile.ZipFile(ipa) as archive:
        if archive.testzip() is not None:
            raise RuntimeError("IPA archive integrity check failed")
        packaged = plistlib.loads(archive.read("Payload/PhiStudio.app/Info.plist"))
        assert packaged["CFBundleExecutable"] == "PhiStudio"
        assert packaged["UIDeviceFamily"] == [1, 2]
        assert not any("embedded.mobileprovision" in n for n in archive.namelist())
    sha = hashlib.sha256(ipa.read_bytes()).hexdigest()
    report = {"file": name, "bytes": ipa.stat().st_size, "sha256": sha,
              "architecture": arch, "platform": "iPhoneOS (device)", "minimum_os": minimum,
              "version": info["CFBundleShortVersionString"], "bundle_id": info["CFBundleIdentifier"],
              "signed": False, "runtime_checked": False,
              "source_commit": run("git", "rev-parse", "HEAD", capture=True),
              "sdk": sdk_version, "xcode": run("xcodebuild", "-version", capture=True)}
    (output / "ipa-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    (output / "macho-load-commands.txt").write_text(load_commands + "\n" + dylibs + "\n")
    (output / "SHA256SUMS.txt").write_text(f"{sha}  {name}\n")
    print(json.dumps(report, indent=2), flush=True)


if __name__ == "__main__":
    main()
