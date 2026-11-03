#!/usr/bin/env python3
"""Check the actual PhiStudio APK, JNI exports, Android ABI and 16 KiB alignment.

Requires LLVM's llvm-readobj, Android SDK aapt2/apksigner/zipalign and the NDK
used by the build. This validates the package and native link, not device runtime.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
from zipfile import ZIP_STORED, ZipFile

API = 26
PAGE = 16384
APP_LIBRARY = "libphistudio_app.so"
REQUIRED_JNI = {
    "JNI_OnLoad", "quad_main",
    *["Java_com_phistudio_rust_MainActivity_" + name for name in (
        "setStorageRoot", "setDisplayDensity", "notifySuspended", "notifyBackRequested", "documentResult")],
    *["Java_quad_1native_QuadNative_" + name for name in (
        "initializeContext", "releaseContext", "activityOnCreate", "activityOnResume",
        "activityOnPause", "activityOnDestroy", "surfaceOnSurfaceCreated",
        "surfaceOnSurfaceDestroyed", "surfaceOnSurfaceChanged", "surfaceOnTouch",
        "surfaceOnKeyDown", "surfaceOnKeyUp", "surfaceOnCharacter")],
}


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def run(*command):
    result = subprocess.run(list(map(str, command)), capture_output=True, text=True)
    require(result.returncode == 0, f"Command failed: {command[0]}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def inspect(readobj, path):
    records = json.loads(run(readobj, "--elf-output-style=JSON", "--file-headers",
                             "--program-headers", "--needed-libs", "--dyn-symbols", path))
    require(len(records) == 1, f"Expected one ELF object: {path}")
    record = records[0]
    symbols = [item["Symbol"] for item in record["DynamicSymbols"]]
    defined = {s["Name"]["Name"].split("@", 1)[0] for s in symbols if s["Section"]["Value"] != 0}
    undefined = {s["Name"]["Name"].split("@", 1)[0] for s in symbols
                 if s["Section"]["Value"] == 0 and s["Binding"]["Name"] == "Global"}
    return record, defined, undefined


def verify_zip_layout(apk, archive):
    """Reject native-library-sized holes left by incremental APK packaging.

    Alignment is stored in local-header extras and counted as live space.
    Allow up to 1 MiB for data descriptors, the APK signing block and padding.
    """
    live = 0
    with apk.open("rb") as stream:
        for item in archive.infolist():
            stream.seek(item.header_offset + 26)
            name_len, extra_len = struct.unpack("<HH", stream.read(4))
            live += 30 + name_len + extra_len + item.compress_size
    unused = archive.start_dir - live
    require(0 <= unused <= 1024 * 1024,
            f"APK contains {unused} bytes outside live ZIP entries before its central directory. "
            "Rebuild with :app:clean :app:assembleDebug before distributing it.")
    return {"size": apk.stat().st_size, "live_local_entries": live,
            "signing_padding_and_unused_bytes": unused}


def verify_java_bridges(archive, sdk, temp):
    """Check declarations in the delivered DEX, including the touch timestamp.

    Exports alone cannot detect a Java/Rust argument mismatch.
    """
    methods = {}
    for name in archive.namelist():
        if not re.fullmatch(r"classes\d*\.dex", name):
            continue
        path = Path(temp) / name
        path.write_bytes(archive.read(name))
        dump = run(sdk / "dexdump", path)
        for block in re.split(r"(?=Class #\d+)", dump):
            found = re.search(r"Class descriptor\s+: '([^']+)'", block)
            if not found:
                continue
            cls = found[1]
            for method in re.finditer(r"name\s+: '([^']+)'\s+type\s+: '([^']+)'\s+access\s+: ([^\n]+)", block):
                methods[(cls, method[1])] = (method[2], method[3])
    quad = {
        "initializeContext": "(Ljava/lang/Object;)V", "releaseContext": "()V",
        "activityOnCreate": "(Ljava/lang/Object;)V", "activityOnResume": "()V",
        "activityOnPause": "()V", "activityOnDestroy": "()V",
        "surfaceOnSurfaceCreated": "(Landroid/view/Surface;)V",
        "surfaceOnSurfaceDestroyed": "(Landroid/view/Surface;)V",
        "surfaceOnSurfaceChanged": "(Landroid/view/Surface;II)V",
        "surfaceOnTouch": "(IIFFJ)V", "surfaceOnKeyDown": "(I)V",
        "surfaceOnKeyUp": "(I)V", "surfaceOnCharacter": "(I)V",
    }
    activity = {
        "setStorageRoot": "(Ljava/lang/String;)V", "setDisplayDensity": "(F)V",
        "notifySuspended": "()V", "notifyBackRequested": "()V",
        "documentResult": "(JILjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
    }
    for cls, expected in [("Lquad_native/QuadNative;", quad), ("Lcom/phistudio/rust/MainActivity;", activity)]:
        for method, descriptor in expected.items():
            actual = methods.get((cls, method))
            require(actual and actual[0] == descriptor and "NATIVE" in actual[1],
                    f"DEX JNI declaration mismatch: {cls}.{method}, expected {descriptor}, got {actual}")
    request = methods.get(("Lcom/phistudio/rust/MainActivity;", "requestPackage"))
    require(request and request[0] == "(J)V" and "STATIC" in request[1],
            "SAF entry method is missing from DEX")
    export = methods.get(("Lcom/phistudio/rust/MainActivity;", "requestExportPackage"))
    require(export and export[0] == "(Ljava/lang/String;Ljava/lang/String;J)V" and "STATIC" in export[1],
            "Package export SAF entry method is missing from DEX")
    media = methods.get(("Lcom/phistudio/rust/MainActivity;", "requestMedia"))
    require(media and media[0] == "(Ljava/lang/String;J)V" and "STATIC" in media[1], "Media SAF entry method is missing from DEX")
    require(("Lcom/phistudio/rust/MainActivity;", "requestDocument") not in methods,
            "Legacy standalone document picker is still present in DEX")
    return {"touch_descriptor": quad["surfaceOnTouch"], "native_methods": len(quad) + len(activity),
            "package_picker_descriptor": request[0], "export_picker_descriptor": export[0], "media_picker_descriptor": media[0]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--ndk", type=Path, required=True)
    parser.add_argument("--build-tools", type=Path, required=True)
    parser.add_argument("--readobj", default=shutil.which("llvm-readobj"))
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    require(args.readobj, "llvm-readobj is required")
    apk = args.apk.resolve(strict=True)
    stubs = args.ndk.resolve() / "toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/26"
    require(stubs.is_dir(), f"Android API {API} stub directory is missing: {stubs}")
    sdk = args.build_tools.resolve()
    badging = run(sdk / "aapt2", "dump", "badging", apk)
    require("package: name='com.phistudio.rust'" in badging, "Wrong Android package ID")
    version = re.search(r"versionCode='(\d+)' versionName='([^']+)'", badging)
    require(version, "APK version name/code are missing")
    require(re.search(r"^(?:minSdkVersion|sdkVersion):'26'$", badging, re.M), "APK minSdk must be 26")
    require(re.search(r"^targetSdkVersion:'35'$", badging, re.M), "APK targetSdk must be 35")
    require("com.phistudio.rust.MainActivity" in badging, "MainActivity is not launchable")
    signature = run(sdk / "apksigner", "verify", "--verbose", "--min-sdk-version", str(API), apk)
    require(re.search(r"^Verified using v[23](?:\.1)? scheme.*: true$", signature, re.M),
            "APK signature verification did not report a modern signing scheme")
    run(sdk / "zipalign", "-c", "-P", "16", "4", apk)
    report = {"apk": str(apk), "sha256": hashlib.file_digest(apk.open("rb"), "sha256").hexdigest(),
              "version_code": int(version[1]), "version_name": version[2],
              "min_sdk": API, "page_size": PAGE, "runtime_checked": False, "native": {}}
    with ZipFile(apk) as archive, tempfile.TemporaryDirectory(prefix="phistudio-apk-") as temp:
        report["zip_layout"] = verify_zip_layout(apk, archive)
        report["java_jni"] = verify_java_bridges(archive, sdk, temp)
        native = [i for i in archive.infolist() if i.filename.startswith("lib/") and i.filename.endswith(".so")]
        require(native, "APK contains no native library (was skipRust used?)")
        require(len({i.filename for i in native}) == len(native), "Duplicate native ZIP entries")
        require(all(i.filename.startswith("lib/arm64-v8a/") for i in native), "Unexpected ABI in APK")
        names = {Path(i.filename).name for i in native}
        require(APP_LIBRARY in names and "libc++_shared.so" in names, "App or libc++ runtime is missing")
        available, unresolved, dependencies = set(), {}, set()
        for item in native:
            name = Path(item.filename).name
            require(item.compress_type == ZIP_STORED, f"Native library is compressed: {name}")
            with apk.open("rb") as stream:
                stream.seek(item.header_offset + 26)
                name_len, extra_len = struct.unpack("<HH", stream.read(4))
            offset = item.header_offset + 30 + name_len + extra_len
            require(offset % PAGE == 0, f"ZIP data for {name} is not 16 KiB aligned")
            library = Path(temp) / name
            library.write_bytes(archive.read(item))
            record, defined, undefined = inspect(args.readobj, library)
            require(record["ElfHeader"]["Machine"]["Value"] == 183, f"{name} is not AArch64")
            require(record["FileSummary"]["Format"] == "elf64-littleaarch64", f"Unexpected ELF format: {name}")
            loads = [p["ProgramHeader"] for p in record["ProgramHeaders"] if p["ProgramHeader"]["Type"]["Value"] == 1]
            require(loads and all(p["Alignment"] >= PAGE and p["Offset"] % PAGE == p["VirtualAddress"] % PAGE for p in loads),
                    f"ELF LOAD segments of {name} are not compatible with 16 KiB pages")
            needed = record.get("NeededLibraries", [])
            require(not any("GLIBC_" in s["Symbol"]["Name"]["Name"] for s in record["DynamicSymbols"]),
                    f"{name} depends on host glibc symbols")
            if name == APP_LIBRARY:
                require(REQUIRED_JNI <= defined, f"Missing JNI entry points: {sorted(REQUIRED_JNI - defined)}")
            available.update(defined)
            unresolved[name] = undefined
            dependencies.update(needed)
            report["native"][name] = {"size": item.file_size, "zip_offset": offset,
                "sha256": hashlib.file_digest(library.open("rb"), "sha256").hexdigest(),
                "load_alignment": [p["Alignment"] for p in loads], "needed": needed,
                "required_jni": sorted(REQUIRED_JNI) if name == APP_LIBRARY else []}
        for dependency in sorted(dependencies - names):
            stub = stubs / dependency
            require(stub.is_file(), f"Library is neither packaged nor available at API {API}: {dependency}")
            _, defined, _ = inspect(args.readobj, stub)
            available.update(defined)
        for name, undefined in unresolved.items():
            missing = undefined - available
            require(not missing, f"{name}: strong undefined symbols missing from API {API} and packaged libraries: {sorted(missing)}")
    report["package_verified"] = True
    report["signature_verified"] = True
    report["native_link_verified"] = True
    report["api_26_symbols_verified"] = True
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"APK verification PASS: {apk}")
    print("arm64-v8a; JNI exports; API 26 native symbols; 16 KiB ELF/ZIP alignment; signature")
    print("Device runtime was not tested by this command.")


if __name__ == "__main__":
    main()
