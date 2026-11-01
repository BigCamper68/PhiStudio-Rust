#!/usr/bin/env python3
"""Reproduce the Preview shader/texture adaptation from build 160 Unity assets.

Requires UnityPy 1.25.4. Input is the assembled sharedassets12.assets file from
UnityDataAssetPack-160.apk (join .split0, .split1, ... in numeric order).
Writes only to the explicit output directory; never changes the source package.
"""
import argparse
import json
from pathlib import Path
import re
import struct


def adapt(source, stage, name):
    code = source.strip().replace("#version 300 es", "#version 100").replace("_Time", "_BlockTime")
    code = re.sub(r"UNITY_LOCATION\(\d+\)\s*", "", code)
    code = re.sub(r"^layout\([^\n]+\) out (?:\w+ )?((?:vec[234]|float)) SV_Target0;", r"\1 SV_Target0;", code, flags=re.M)
    code = code.replace("texture(", "texture2D(").replace("textureLod(", "texture2DLodEXT(")
    if stage == "frag":
        code = re.sub(r"^in ", "varying ", code, flags=re.M)
        if name == "ActiveBlock_0":
            # Macroquad's uniform interface has no array descriptor. Preserve
            # all ten positions with scalar uniforms and a GLES2-safe selector.
            declarations = "\n".join(f"uniform vec4 _TouchPos{i};" for i in range(10))
            selector = "\n".join(f"    if (index == {i}) return _TouchPos{i};" for i in range(9))
            code = re.sub(r"uniform\s+vec4 _TouchPos\[10\];", declarations, code)
            code = re.sub(r"_TouchPos\[([^\]]+)\]", r"touchPosition(\1)", code)
            code = code.replace("void main()", "vec4 touchPosition(int index) {\n" + selector + "\n    return _TouchPos9;\n}\nvoid main()")
            code = code.replace("_TouchDisplaceMap", "_DisplaceMap")
            for sampler, channel in [("_ComposeRT", "r"), ("_ReadyComposeRT", "g"), ("_DisabledNormalBlockRT", "b"), ("_DisabledSubtractBlockRT", "a")]:
                code = re.sub(r"uniform\s+(?:mediump )?sampler2D " + sampler + r";", "", code)
                component = "y" if sampler == "_DisabledSubtractBlockRT" else "x"
                code = re.sub(r"texture2D\(" + sampler + r", ([^;\n]+?)\)\." + component, r"texture2D(_MaskRT, \1)." + channel, code)
            code = code.replace("void main()", "uniform sampler2D _MaskRT;\nvoid main()")
            seen = False
            lines = []
            for line in code.splitlines():
                if "sampler2D _DisplaceMap;" in line:
                    if seen:
                        continue
                    seen = True
                lines.append(line)
            code = "\n".join(lines)
            # Use Macroquad's primary texture slot for the shared displacement
            # map. Six extra samplers + its two reserved slots fit GLES2's eight.
            code = re.sub(r"\b_DisplaceMap\b", "Texture", code)
        code = re.sub(r"texture2DLodEXT\((\w+), ([^\n;]+), 0.0\)", r"texture2D(\1, \2)", code)
        output = "SV_Target0"
        if "vec2 SV_Target0;" in code:
            output = "vec4(SV_Target0, 0.0, 1.0)"
        elif "float SV_Target0;" in code:
            output = "vec4(SV_Target0, 0.0, 0.0, 1.0)"
        code = code.replace("    return;", f"    gl_FragColor = {output};\n    return;")
    else:
        code = re.sub(r"^in ", "attribute ", code, flags=re.M)
        code = re.sub(r"^out ", "varying ", code, flags=re.M)
        code = code.replace("attribute highp vec4 in_POSITION0;", "attribute vec3 position;\n#define in_POSITION0 vec4(position, 1.0)")
        code = code.replace("attribute highp vec4 in_COLOR0;", "attribute vec4 color0;\n#define in_COLOR0 (color0 / 255.0)")
        code = code.replace("attribute highp vec2 in_TEXCOORD0;", "attribute vec2 texcoord;\n#define in_TEXCOORD0 texcoord")
        start = code.find("    u_xlat0 = in_POSITION0.yyyy")
        if start != -1:
            end = code.find("    u_xlat1.x = u_xlat0.y", start)
            if end != -1:
                code = code[:start] + "    u_xlat0 = in_POSITION0;\n" + code[end:]
            else:
                end = code.index("\n", code.index("    gl_Position = ", start))
                code = code[:start] + "    gl_Position = in_POSITION0;\n" + code[end + 1:]
    return code + "\n"


def extract(source, output):
    import UnityPy
    from PIL import Image
    from UnityPy.export.ShaderConverter import CompressionHelper

    (output / "shaders").mkdir(parents=True, exist_ok=True)
    (output / "assets").mkdir(exist_ok=True)
    props = {}
    texture_names = {"BlockNoise1", "PointNoise", "FD_Noise_00000", "Round10_Blur4"}
    for obj in UnityPy.load(str(source)).objects:
        if obj.type.name == "Shader":
            data = obj.read()
            name = data.m_ParsedForm.m_Name.split("/")[-1]
            if name not in {"ActiveBlock", "DisabledBlock", "EdgeMask", "BlockCompose", "SubtractBlockBlender"}:
                continue
            assert data.platforms == [9], "Expected GLES3 build 160 shader programs"
            offset, length = data.offsets[0][0], data.compressedLengths[0][0]
            blob = CompressionHelper.decompress_lz4(bytes(data.compressedBlob)[offset:offset + length], data.decompressedLengths[0][0])
            for index, match in enumerate(re.finditer(rb"#version[^\0]+", blob)):
                if name == "EdgeMask" and index == 0:
                    continue
                vertex, fragment = match.group().decode().split("#endif\n#ifdef FRAGMENT\n")
                fragment = fragment.rsplit("#endif", 1)[0]
                for stage, code in [("vert", vertex), ("frag", fragment)]:
                    (output / "shaders" / f"{name}_{index}.{stage}").write_text(adapt(code, stage, f"{name}_{index}"))
        elif obj.type.name == "Texture2D":
            data = obj.read()
            if data.m_Name in texture_names:
                data.image.transpose(Image.Transpose.FLIP_TOP_BOTTOM).save(output / "assets" / (data.m_Name + ".png"))
        elif obj.type.name == "Sprite" and obj.path_id == 60:
            sprite = obj.read_typetree()
            data = sprite["m_RD"]
            count = data["m_VertexData"]["m_VertexCount"]
            vertices = list(struct.iter_unpack("<fff", data["m_VertexData"]["m_DataSize"][:count * 12]))
            transform = data["uvTransform"]
            record = {
                "vertices": [[x, y, (x * transform["x"] + transform["y"]) / 44., (y * transform["z"] + transform["w"]) / 44.] for x, y, _ in vertices],
                "indices": list(struct.unpack("<12H", bytes(data["m_IndexBuffer"])))
            }
            (output / "assets/hover-sprite.json").write_text(json.dumps(record, indent=2) + "\n")
        elif obj.type.name == "Material" and obj.path_id in [5, 6, 8, 10, 12]:
            values = obj.read_typetree()["m_SavedProperties"]
            record = dict(values["m_Floats"])
            for key, value in values["m_Colors"]:
                record[key] = list(value.values())
            for key, value in values["m_TexEnvs"]:
                record[key + "_ST"] = [value["m_Scale"]["x"], value["m_Scale"]["y"], value["m_Offset"]["x"], value["m_Offset"]["y"]]
            props[str(obj.path_id)] = record
    (output / "assets/materials.json").write_text(json.dumps(props, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assets", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    extract(args.assets, args.output)
