#!/usr/bin/env python3
"""Import/render/edit/save local reference packages without changing their inputs.

Run under Xvfb after building phistudio-app. User packages stay outside Git;
reports, screenshots and isolated workspaces go to artifacts/checks/import-corpus.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("archives", nargs="+", type=Path)
parser.add_argument("--output", type=Path, default=ROOT / "artifacts/checks/import-corpus")
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


reports = []
for index, archive in enumerate(args.archives):
    archive = archive.resolve()
    before = digest(archive)
    directory = args.output / f"chart-{index}"
    directory.mkdir(exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix="workspace-", dir=directory))
    with (directory / "editor.log").open("w") as log:
        process = subprocess.run([ROOT / "target/debug/phistudio", "--workspace", workspace,
                                  "--package", archive, "--smoke-test", directory],
                                 cwd=ROOT, stdout=log, stderr=log, timeout=120)
    assert digest(archive) == before, f"Input archive was modified: {archive.name}"
    report = {"name": archive.name, "sha256": before, "exit_code": process.returncode,
              "input_unchanged": True, "workspace": str(workspace)}
    if process.returncode:
        print(archive.name, "FAILED", flush=True)
        print((directory / "editor.log").read_text()[-4000:], flush=True)
    else:
        report["smoke"] = (directory / "smoke-test.txt").read_text()
        report["resource_warning"] = (directory / "resource-warning.txt").read_text()
        print(archive.name, "PASS", flush=True)
        print(report["smoke"], flush=True)
    reports.append(report)
(args.output / "report.json").write_text(json.dumps(reports, indent=2, ensure_ascii=False) + "\n")
raise SystemExit(int(any(report["exit_code"] != 0 for report in reports)))
