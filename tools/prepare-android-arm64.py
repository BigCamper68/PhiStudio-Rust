#!/usr/bin/env python3
"""Prepare an NDK view for an ARM64 Linux build host.

The official NDK remains untouched. Its Android sysroot, libc++ and compiler-rt
are reused; only the host executables are provided by the local LLVM install.
Pass the output directory to cargo-ndk via ANDROID_NDK_HOME.
"""
import argparse
import json
from pathlib import Path
import platform
import shlex
import shutil
import subprocess


def executable(name):
    path = shutil.which(name)
    if not path:
        raise SystemExit(f"Missing host tool: {name}")
    return Path(path).absolute()


def link(source, destination):
    if destination.is_symlink():
        if destination.resolve() == source.resolve():
            return
        destination.unlink()
    elif destination.exists():
        raise SystemExit(f"Refusing to replace an existing file: {destination}")
    destination.symlink_to(source, target_is_directory=source.is_dir())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ndk", type=Path, required=True, help="Unmodified, extracted official Linux NDK")
    parser.add_argument("--output", type=Path, required=True, help="New directory for the ARM64 host view")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("aarch64", "arm64"):
        parser.error("This helper is for ARM64 Linux hosts; use the stock NDK on supported hosts")
    source = args.ndk.resolve(strict=True)
    output = args.output.resolve()
    prebuilt = source / "toolchains/llvm/prebuilt/linux-x86_64"
    if not (source / "source.properties").is_file() or not (prebuilt / "sysroot/usr/include/android").is_dir():
        parser.error("The supplied directory is not an extracted Linux NDK")
    if output == source or output.is_relative_to(source):
        parser.error("The host view must live outside the original NDK")
    clang = executable("clang")
    clangxx = executable("clang++")
    linker = executable("ld.lld")
    tools = {name: executable(name) for name in ("llvm-ar", "llvm-ranlib", "llvm-strip", "llvm-readelf", "llvm-nm", "llvm-objcopy")}
    resources = [p for p in (prebuilt / "lib/clang").iterdir() if p.is_dir() and p.name.split('.')[0].isdigit()]
    resource = max(resources, key=lambda p: tuple(map(int, p.name.split('.'))))
    manifest = {
        "format": 1,
        "source_ndk": str(source),
        "source_properties": (source / "source.properties").read_text(),
        "host": platform.machine(),
        "clang": str(clang),
        "clang_version": subprocess.check_output([clang, "--version"], text=True).splitlines()[0],
        "resource_dir": str(resource),
        "linker": str(linker),
    }
    marker = output / "phistudio-host.json"
    if output.exists() and any(output.iterdir()) and not marker.is_file():
        parser.error("Refusing to write into a nonempty directory without a PhiStudio host marker")
    output.mkdir(parents=True, exist_ok=True)
    if marker.is_file():
        previous = json.loads(marker.read_text())
        if previous.get("source_ndk") != str(source):
            parser.error("The existing host view refers to a different NDK; choose a new output directory")
    # Write the ownership marker before creating files, so an interrupted setup
    # can be resumed without treating unrelated directories as disposable.
    marker.write_text(json.dumps(manifest, indent=2) + "\n")
    for name in ("source.properties", "meta", "build", "NOTICE", "NOTICE.toolchain"):
        if (source / name).exists():
            link(source / name, output / name)
    host = output / "toolchains/llvm/prebuilt/linux-x86_64"
    host.mkdir(parents=True, exist_ok=True)
    for name in ("sysroot", "lib", "include", "share"):
        if (prebuilt / name).exists():
            link(prebuilt / name, host / name)
    binaries = host / "bin"
    binaries.mkdir(exist_ok=True)
    for name, tool in tools.items():
        link(tool, binaries / name)
    link(linker, binaries / "ld.lld")
    for name, compiler in (("clang", clang), ("clang++", clangxx)):
        # Use an actual script rather than a symlink: the host driver must never
        # discover glibc's headers or libstdc++ while building Android objects.
        flags = [str(compiler), f"--sysroot={prebuilt / 'sysroot'}", f"-resource-dir={resource}", f"-fuse-ld={linker}"]
        wrapper = binaries / name
        if wrapper.is_symlink():
            wrapper.unlink()
        wrapper.write_text("#!/bin/sh\nexec " + " ".join(map(shlex.quote, flags)) + ' "$@"\n')
        wrapper.chmod(0o755)
    print(f"Prepared ARM64 Linux host view: {output}")
    print(f"Android sysroot: {prebuilt / 'sysroot'}")
    print(f"Host compiler: {manifest['clang_version']}")
    print("Original NDK files were not changed.")


if __name__ == "__main__":
    main()
