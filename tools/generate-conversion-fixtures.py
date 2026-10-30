#!/usr/bin/env python3
"""Generate/verify conversion fixtures using unchanged Java PhiStudio sources."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/phistudio-core/tests/fixtures/conversion"
COMMIT = "bb2e9b34520a97b10e9012c26101aec061b9a004"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--reference", required=True, type=Path)
parser.add_argument("--json-jar", required=True, type=Path, help="org.json:json:20240303")
parser.add_argument("--verify", action="store_true")
args = parser.parse_args()
assert subprocess.check_output(["git", "-C", args.reference, "rev-parse", "HEAD"], text=True).strip() == COMMIT
sources = args.reference / "app/src/main/java"
java_files = ["packageio/OfficialPhigrosConverter.java", "packageio/LegacyPecConverter.java", "model/ChartDocument.java", "model/BeatTime.java", "model/LineEvent.java"]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def canonical(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"

manifest = {"reference": "https://github.com/BigCamper68/PhiStudio", "commit": COMMIT,
            "json_dependency": "org.json:json:20240303", "java_sources": {}, "fixtures": {}}
for name in java_files:
    manifest["java_sources"]["app/src/main/java/com/xpe/mobile/" + name] = sha(sources / "com/xpe/mobile" / name)

with tempfile.TemporaryDirectory(prefix="phistudio-java-reference-") as temporary:
    temporary = Path(temporary)
    classes = temporary / "classes"
    classes.mkdir()
    driver = temporary / "ConversionReference.java"
    driver.write_text('''package com.xpe.mobile.packageio;
import java.nio.file.*;
import com.xpe.mobile.model.ChartDocument;
import org.json.JSONObject;
public class ConversionReference {
 public static void main(String[] args) throws Exception {
  String source=Files.readString(Path.of(args[1]));
  ChartDocument chart=args[0].equals("pec") ? LegacyPecConverter.convert(source) : OfficialPhigrosConverter.convert(new JSONObject(source));
  Files.writeString(Path.of(args[2]),chart.toJsonString());
 }
}''')
    subprocess.run(["javac", "--release", "17", "-d", classes, "-cp", args.json_jar,
                    "-sourcepath", sources, driver,
                    sources / "com/xpe/mobile/packageio/OfficialPhigrosConverter.java",
                    sources / "com/xpe/mobile/packageio/LegacyPecConverter.java"], check=True)
    for name, extension in [("official-v1", "json"), ("official-v3", "json"), ("pec-minimum", "pec"), ("pec-overlap", "pec")]:
        source = FIXTURES / f"{name}.{extension}"
        generated = temporary / f"{name}.json"
        subprocess.run(["java", "-cp", f"{classes}:{args.json_jar}", "com.xpe.mobile.packageio.ConversionReference",
                        "pec" if extension == "pec" else "official", source, generated], check=True)
        expected = FIXTURES / f"{name}.expected.rpe.json"
        text = canonical(json.loads(generated.read_text()))
        if args.verify:
            assert expected.read_text() == text, expected.name
        else:
            expected.write_text(text)
        manifest["fixtures"][source.name] = {"source_sha256": sha(source), "expected": expected.name,
                                              "expected_sha256": hashlib.sha256(text.encode()).hexdigest()}
    provenance = FIXTURES / "SOURCE.json"
    if args.verify:
        assert json.loads(provenance.read_text()) == manifest
    else:
        provenance.write_text(canonical(manifest))
print("PASS: all four fixtures match the original Java converters at", COMMIT[:7])
