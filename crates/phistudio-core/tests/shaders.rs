use phistudio_core::{shaders::*, *};
use serde_json::json;
#[test]
fn preserved_shader_override_exports_standard_sidecar_without_changing_originals() {
    let root = tempfile::tempdir().unwrap();
    let extra = json!({"bpm":120,"effects":[{"start":[0,0,1],"end":[4,0,1],"shader":"/source.glsl","unknownEffect":[8,9]}],"unknownRoot":{"keep":true}});
    let bytes = serde_json::to_vec(&extra).unwrap();
    std::fs::write(root.path().join("extra.json"), &bytes).unwrap();
    std::fs::write(root.path().join("source.glsl"), b"// original shader\n").unwrap();
    let doc = ChartDocument::empty("Shaders", 120.).unwrap();
    assert_eq!(load_shader_extra(&doc, Some(root.path())).unwrap(), extra);
    let mut raw = doc.as_json().clone();
    let mut modified = extra.clone();
    modified["effects"][0]["global"] = json!(true);
    raw[EXTRA_KEY] = modified.clone();
    let doc = ChartDocument::parse(&serde_json::to_string(&raw).unwrap()).unwrap();
    let export =
        prepare_project_export(&doc, &ProjectOptions::default(), Some(root.path()), None).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(export.path()).unwrap()).unwrap();
    let exported: serde_json::Value =
        serde_json::from_reader(zip.by_name("extra.json").unwrap()).unwrap();
    assert_eq!(exported, modified);
    assert_eq!(zip.file_names().filter(|n| *n == "extra.json").count(), 1);
    assert_eq!(
        std::fs::read(root.path().join("extra.json")).unwrap(),
        bytes
    );
    assert_eq!(
        doc.as_json()[EXTRA_KEY]["unknownRoot"],
        extra["unknownRoot"]
    );
}
#[test]
fn native_shader_input_rejects_empty_tempos_and_escaping_paths() {
    assert!(validate_shader_extra(&json!({"bpm":[],"effects":[]})).is_err());
    let extra =
        json!({"bpm":120,"effects":[{"start":[0,0,1],"end":[1,0,1],"shader":"/../outside.glsl"}]});
    assert!(validate_shader_extra(&extra).is_err());
}

fn effect(tag: &str) -> serde_json::Value {
    json!({"start":[0,0,1],"end":[4,0,1],"shader":"grayscale","unknown":tag})
}
fn command(baseline: &serde_json::Value, action: ShaderAction) -> Command {
    Command::EditShader {
        baseline: baseline.clone(),
        action,
    }
}
#[test]
fn shader_sidecar_hydration_sparse_edits_reorder_and_exact_undo() {
    let doc = ChartDocument::empty("Shader history", 120.).unwrap();
    let original = doc.as_json().clone();
    let baseline = json!({"bpm":120,"effects":[effect("a"),effect("b")],"videos":[{"path":"kept.mp4"}],"unknown":{"yes":true}});
    let mut editor = Editor::new(doc);
    let mut changed = effect("a");
    changed["global"] = json!(true);
    assert!(
        editor
            .execute(command(
                &baseline,
                ShaderAction::Update {
                    index: 0,
                    effect: changed.clone()
                }
            ))
            .unwrap()
    );
    assert_eq!(
        editor.document().as_json()[EXTRA_KEY]["videos"],
        baseline["videos"]
    );
    assert_eq!(
        editor.document().as_json()["judgeLineList"],
        original["judgeLineList"]
    );
    editor.mark_saved();
    let saved = editor.document().as_json().clone();
    editor
        .execute(command(
            &baseline,
            ShaderAction::Insert {
                index: 1,
                effect: changed,
            },
        ))
        .unwrap();
    editor
        .execute(command(&baseline, ShaderAction::Move { from: 0, to: 2 }))
        .unwrap();
    editor
        .execute(command(&baseline, ShaderAction::Delete(1)))
        .unwrap();
    for _ in 0..3 {
        assert!(editor.undo());
    }
    assert_eq!(editor.document().as_json(), &saved);
    assert!(!editor.is_dirty());
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &original);
    assert!(editor.redo());
    assert_eq!(editor.document().as_json(), &saved);
    for _ in 0..3 {
        assert!(editor.redo());
    }
    assert_eq!(
        editor.document().as_json()[EXTRA_KEY]["effects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn shader_invalid_transaction_restores_sources_history_and_missing_containers() {
    let doc = ChartDocument::empty("Atomic", 120.).unwrap();
    let baseline = default_shader_extra(&doc);
    let original = doc.as_json().clone();
    let mut editor = Editor::new(doc);
    editor
        .execute(command(
            &baseline,
            ShaderAction::Insert {
                index: 0,
                effect: effect("new"),
            },
        ))
        .unwrap();
    editor.undo();
    let revision = editor.revision();
    let result = editor.execute_batch(
        "Shader import",
        vec![
            Command::SetShaderSource {
                path: "shaders/custom.glsl".into(),
                source: "void main() {}".into(),
            },
            command(&baseline, ShaderAction::Delete(9)),
        ],
    );
    assert!(result.is_err());
    assert_eq!(editor.document().as_json(), &original);
    assert_eq!(editor.revision(), revision);
    assert!(editor.can_redo());
    assert!(!editor.is_dirty());
    let mut raw = original.clone();
    raw[EXTRA_KEY] = json!({"bpm":120});
    let mut editor = Editor::new(ChartDocument::parse(&raw.to_string()).unwrap());
    editor
        .execute(command(
            &baseline,
            ShaderAction::Insert {
                index: 0,
                effect: effect("x"),
            },
        ))
        .unwrap();
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
#[test]
fn source_edits_export_recover_and_never_overwrite_original_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("custom.glsl"), "// original").unwrap();
    let doc = ChartDocument::empty("Source", 120.).unwrap();
    let baseline = default_shader_extra(&doc);
    let mut editor = Editor::new(doc);
    let mut value = effect("with-source");
    value["shader"] = json!("/custom.glsl");
    editor
        .execute_batch(
            "Create custom effect",
            vec![
                Command::SetShaderSource {
                    path: "custom.glsl".into(),
                    source: "// working\nvoid main() {}".into(),
                },
                command(
                    &baseline,
                    ShaderAction::Insert {
                        index: 0,
                        effect: value,
                    },
                ),
            ],
        )
        .unwrap();
    let saved = editor.document().to_json().unwrap();
    let recovered = ChartDocument::parse(&saved).unwrap();
    assert_eq!(
        read_shader_source(&recovered, Some(root.path()), "custom.glsl").unwrap(),
        "// working\nvoid main() {}"
    );
    let export = prepare_project_export(
        &recovered,
        &ProjectOptions::default(),
        Some(root.path()),
        None,
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(export.path()).unwrap()).unwrap();
    assert_eq!(zip.file_names().filter(|p| *p == "custom.glsl").count(), 1);
    let mut source = String::new();
    use std::io::Read;
    zip.by_name("custom.glsl")
        .unwrap()
        .read_to_string(&mut source)
        .unwrap();
    assert_eq!(source, "// working\nvoid main() {}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("custom.glsl")).unwrap(),
        "// original"
    );
    editor.undo();
    assert!(editor.document().as_json().get(SOURCES_KEY).is_none());
    assert!(editor.document().as_json().get(EXTRA_KEY).is_none());
    editor.redo();
    assert_eq!(editor.document().to_json().unwrap(), saved);
}
#[test]
fn source_boundaries_and_native_uniform_discovery() {
    for path in [
        "../bad.glsl",
        "/absolute.glsl",
        "a\\..\\bad.glsl",
        "chart.json",
        "info.yml",
        "extra.json",
    ] {
        assert!(
            validate_shader_source(path, "void main(){}").is_err(),
            "{path}"
        );
    }
    let source = "// uniform float ignored;\n/* uniform float hidden; */\nuniform float amount; // %0.5%\nuniform highp vec2 center; // %0.2,0.8%\nuniform vec4 tint; // %1,0.5,0,1%\nuniform float time;\nuniform vec2 screenSize;\nuniform sampler2D screenTexture;";
    let p = shader_parameters(source);
    assert_eq!(p.len(), 3);
    assert_eq!(p[0].default, json!(0.5));
    assert_eq!(p[1].default, json!([0.2, 0.8]));
    assert_eq!(p[2].default, json!([255, 128, 0, 255]));
}
#[test]
fn shader_clock_retains_independent_bpm_and_uniform_metadata() {
    let doc = ChartDocument::empty("Tempo", 180.).unwrap();
    let mut baseline = json!({"bpm":[{"time":[0,0,1],"bpm":120},{"time":[4,0,1],"bpm":60}],"effects":[effect("native")]});
    baseline["effects"][0]["vars"] = json!({"amount":[{"startTime":[1,0,1],"endTime":[6,0,1],"start":0.1,"end":0.9,"easingType":6,"easingLeft":0.2,"easingRight":0.8,"unknownKey":42}],"center":[0.5,0.5],"color":[255,0,0,128]});
    validate_shader_extra(&baseline).unwrap();
    let tempo = shader_tempo(&baseline).unwrap();
    assert_eq!(tempo.seconds_at(6.), 4.);
    assert_eq!(tempo.beat_at(4.), 6.);
    let mut editor = Editor::new(doc);
    editor
        .execute(command(&baseline, ShaderAction::Tempo(json!(100))))
        .unwrap();
    assert_eq!(
        editor.document().as_json()[EXTRA_KEY]["effects"],
        baseline["effects"]
    );
    editor.undo();
    assert!(editor.document().as_json().get(EXTRA_KEY).is_none());
}
