//! GPU check for authored source overlays, animation, compiler recovery and Undo.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, Command, Editor, ProjectOptions, shaders::*};
use phistudio_prpr::Preview;
use serde_json::json;
const SOURCE: &str = "#version 100\nprecision mediump float;\nvarying vec2 uv;\nuniform sampler2D screenTexture;\nuniform float amount; // %0.5%\nuniform vec2 center; // %0.5,0.5%\nuniform vec4 tint; // %1,1,1,1%\nvoid main(){gl_FragColor=vec4(amount,center.x,tint.b,1.0);}";
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Shader authoring regression".into(),
            window_width: 1280,
            window_height: 720,
            ..Default::default()
        },
        async {
            if let Err(error) = pollster::block_on(run()) {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
async fn run() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/shader-authoring-gpu".into()),
    );
    std::fs::create_dir_all(&out)?;
    let mut editor = Editor::new(ChartDocument::empty("Shader authoring", 120.)?);
    let baseline = default_shader_extra(editor.document());
    let effect = json!({"start":[0,0,1],"end":[8,0,1],"shader":"/shaders/test.glsl","global":true,"vars":{"amount":[{"startTime":[0,0,1],"endTime":[4,0,1],"start":0.,"end":1.,"easingType":1}],"center":[0.4,0.6],"tint":[255,128,64,255]}});
    editor.execute_batch(
        "Author shader",
        vec![
            Command::SetShaderSource {
                path: "shaders/test.glsl".into(),
                source: SOURCE.into(),
            },
            Command::EditShader {
                baseline,
                action: ShaderAction::Insert { index: 0, effect },
            },
        ],
    )?;
    let options = ProjectOptions::default();
    let mut preview = Preview::new(editor.document(), None, &options).await?;
    ensure!(
        preview.shader_report()["warning"].is_null(),
        "{:?}",
        preview.shader_report()
    );
    ensure!(preview.shader_report()["global"] == 1);
    preview.render(0.5, preview.target_viewport());
    let before = preview.snapshot();
    before.export_png(out.join("animated-early.png").to_str().unwrap());
    preview.render(1.5, preview.target_viewport());
    let after = preview.snapshot();
    after.export_png(out.join("animated-late.png").to_str().unwrap());
    ensure!(
        before.bytes != after.bytes,
        "Animated float did not reach native material"
    );
    let at = (after.width as usize * after.height as usize / 2 + after.width as usize / 2) * 4;
    let pixel = &after.bytes[at..at + 4];
    ensure!(
        pixel[0] > 170
            && (i32::from(pixel[1]) - 102).abs() < 3
            && (i32::from(pixel[2]) - 64).abs() < 3,
        "Unexpected native uniform pixel {pixel:?}"
    );
    editor.execute(Command::SetShaderSource {
        path: "shaders/test.glsl".into(),
        source: "this is deliberately invalid GLSL".into(),
    })?;
    preview
        .replace_chart(editor.document(), None, &options)
        .await?;
    ensure!(preview.shader_report()["warning"].is_string());
    preview.render(1.5, preview.target_viewport());
    let failed = preview.snapshot();
    ensure!(
        after.bytes == failed.bytes,
        "Failed compile discarded last working effects"
    );
    editor.undo();
    preview
        .replace_chart(editor.document(), None, &options)
        .await?;
    ensure!(preview.shader_report()["warning"].is_null());
    preview.render(1.5, preview.target_viewport());
    ensure!(
        preview.snapshot().bytes == after.bytes,
        "Source Undo did not restore rendering"
    );
    editor.undo();
    preview
        .replace_chart(editor.document(), None, &options)
        .await?;
    ensure!(preview.shader_report()["global"] == 0);
    std::fs::write(
        out.join("result.txt"),
        "PASS: authored GLSL overlay, animated float, vec2/RGBA, compiler recovery, exact source Undo and effect removal\n",
    )?;
    println!("PASS: native shader authoring pipeline");
    Ok(())
}
