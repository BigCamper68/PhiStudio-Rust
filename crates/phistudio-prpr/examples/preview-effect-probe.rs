//! Pixel-level check of transient plugin passes on the original prpr scene.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, EditorOptions, ProjectOptions};
use phistudio_prpr::{Preview, PreviewFilter};
use phistudio_sdk::{PreviewPass, PreviewShader, ShaderUniform};
use std::{collections::BTreeMap, path::Path};

const SOURCE: &str = "#version 100\nprecision mediump float;\nvarying vec2 uv;\nuniform sampler2D screenTexture;\nuniform float amount; // %0.5%\nuniform vec4 tint; // %0,1,1,1%\nvoid main(){gl_FragColor=mix(texture2D(screenTexture,uv),tint,amount);}";
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Plugin preview effect probe".into(),
            window_width: 640,
            window_height: 360,
            ..Default::default()
        },
        async {
            if let Err(error) = run().await {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
fn filter(
    id: &str,
    shader: PreviewShader,
    uniforms: BTreeMap<String, ShaderUniform>,
) -> PreviewFilter {
    PreviewFilter {
        plugin: "probe.preview".into(),
        generation: 1,
        contribution: "grade".into(),
        pass: PreviewPass {
            id: id.into(),
            shader,
            uniforms,
        },
    }
}
fn sample(preview: &mut Preview, directory: &Path, name: &str) -> [u8; 4] {
    preview.render(0.5, preview.target_viewport());
    let image = preview.snapshot();
    image.export_png(directory.join(name).to_string_lossy().as_ref());
    let at = (10 * image.width as usize + 10) * 4;
    [
        image.bytes[at],
        image.bytes[at + 1],
        image.bytes[at + 2],
        image.bytes[at + 3],
    ]
}
async fn run() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/plugin-preview-effects".into()),
    );
    std::fs::create_dir_all(&directory)?;
    next_frame().await;
    let document = ChartDocument::empty("Transient effects", 120.)?;
    let original = document.as_json().clone();
    let mut preview = Preview::new(&document, None, &ProjectOptions::default()).await?;
    let options = EditorOptions {
        preview_shaders: true,
        preview_hud: false,
        preview_particles: false,
        ..Default::default()
    };
    preview.set_editor_options(&options, 0)?;
    let baseline = sample(&mut preview, &directory, "baseline.png");
    ensure!(
        baseline[3] == 255,
        "Baseline renderer/readback is unavailable: {baseline:?}"
    );
    let mut first = filter(
        "tint",
        PreviewShader::Fragment {
            source: SOURCE.into(),
        },
        BTreeMap::from([
            ("amount".into(), ShaderUniform::Float(0.)),
            ("tint".into(), ShaderUniform::Vec4([0., 1., 1., 1.])),
        ]),
    );
    ensure!(
        preview
            .set_plugin_effects(std::slice::from_ref(&first))
            .is_empty()
    );
    let unchanged = sample(&mut preview, &directory, "amount-zero.png");
    ensure!(
        baseline == unchanged,
        "Zero amount changed the input pixel: {baseline:?} -> {unchanged:?}"
    );
    first
        .pass
        .uniforms
        .insert("amount".into(), ShaderUniform::Float(1.));
    ensure!(
        preview
            .set_plugin_effects(std::slice::from_ref(&first))
            .is_empty()
    );
    let tinted = sample(&mut preview, &directory, "amount-one.png");
    let active = first.clone();
    ensure!(
        tinted[0] < 3 && tinted[1] > 252 && tinted[2] > 252 && tinted[3] == 255,
        "Live uniforms did not reach prpr: {tinted:?}"
    );
    let second = filter(
        "gray",
        PreviewShader::Preset {
            name: "grayscale".into(),
        },
        BTreeMap::from([("factor".into(), ShaderUniform::Float(1.))]),
    );
    ensure!(preview.set_plugin_effects(&[first, second]).is_empty());
    let chained = sample(&mut preview, &directory, "chained.png");
    ensure!(
        (i32::from(chained[0]) - i32::from(chained[1])).abs() < 3
            && (i32::from(chained[1]) - i32::from(chained[2])).abs() < 3
            && chained[0] > 100,
        "Pass ordering/target swap failed: {chained:?}"
    );
    let uniform_source = "#version 100\nprecision mediump float;\nuniform float amount;\nuniform vec2 center;\nuniform vec4 tint;\nvoid main(){gl_FragColor=vec4(amount,center.x,tint.b,1.0);}";
    let uniforms = filter(
        "uniform-values",
        PreviewShader::Fragment {
            source: uniform_source.into(),
        },
        BTreeMap::from([
            ("amount".into(), ShaderUniform::Float(0.25)),
            ("center".into(), ShaderUniform::Vec2([0.4, 0.8])),
            ("tint".into(), ShaderUniform::Vec4([0.2, 0.8, 0.6, 1.])),
        ]),
    );
    ensure!(preview.set_plugin_effects(&[uniforms]).is_empty());
    let uniform_values = sample(&mut preview, &directory, "uniform-values.png");
    for (actual, expected) in uniform_values.into_iter().zip([64, 102, 153, 255]) {
        ensure!(
            (i32::from(actual) - expected).abs() < 3,
            "Float/vec2/vec4 binding failed: {uniform_values:?}"
        );
    }
    let bad = filter(
        "invalid",
        PreviewShader::Fragment {
            source: "deliberately invalid GLSL".into(),
        },
        BTreeMap::new(),
    );
    ensure!(preview.set_plugin_effects(std::slice::from_ref(&bad)).len() == 1);
    ensure!(preview.set_plugin_effects(&[bad]).len() == 1);
    let failed = sample(&mut preview, &directory, "failed.png");
    ensure!(
        failed == baseline,
        "Failed pass affected the chart: {failed:?}"
    );
    ensure!(preview.set_plugin_effects(&[]).is_empty());
    let cleared = sample(&mut preview, &directory, "cleared.png");
    ensure!(cleared == baseline && document.as_json() == &original);
    let mut native = original.clone();
    native[phistudio_core::shaders::EXTRA_KEY] = serde_json::json!({"bpm":120,"effects":[{"start":[0,0,1],"end":[8,0,1],"shader":"grayscale","global":true,"vars":{"factor":1.0}}]});
    let native = ChartDocument::from_value(native)?;
    preview
        .replace_chart(&native, None, &ProjectOptions::default())
        .await?;
    ensure!(preview.shader_report()["global"] == 1 && preview.shader_report()["warning"].is_null());
    let native_baseline = sample(&mut preview, &directory, "native-baseline.png");
    ensure!(
        (i32::from(native_baseline[0]) - i32::from(native_baseline[1])).abs() < 2
            && native_baseline != baseline
    );
    ensure!(preview.set_plugin_effects(&[active]).is_empty());
    ensure!(sample(&mut preview, &directory, "native-plus-plugin.png") == tinted);
    preview.set_editor_options(
        &EditorOptions {
            preview_shaders: false,
            ..options.clone()
        },
        0,
    )?;
    ensure!(sample(&mut preview, &directory, "shaders-off.png") == baseline);
    preview.set_editor_options(&options, 0)?;
    ensure!(sample(&mut preview, &directory, "shaders-on.png") == tinted);
    ensure!(preview.set_plugin_effects(&[]).is_empty());
    ensure!(sample(&mut preview, &directory, "native-after-clear.png") == native_baseline);
    ensure!(preview.shader_report()["global"] == 1);
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"result":"PASS","baseline":baseline,"tinted":tinted,"chained":chained,"cleared":cleared,"native_baseline":native_baseline,"uniform_values":uniform_values}),
        )?,
    )?;
    println!(
        "PASS: prpr transient custom/preset passes, live uniforms, pass order, compile failure and removal"
    );
    Ok(())
}
