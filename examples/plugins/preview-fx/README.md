# Preview FX

An SDK-only Wasm example using transient post-processing in the original prpr
preview pipeline. It leaves existing Tools, chart data and export behavior intact.

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release --locked
cp target/wasm32-unknown-unknown/release/phistudio_example_preview_fx.wasm plugin.wasm
```

Install and enable this directory in **Projects → Plugins**. Open its panel
from Windows. The panel controls a grayscale
preset or a custom GLSL pass with float/vec2/vec4 uniforms and the live `time`
uniform. Changes to values reuse the GPU program. **Test a shader compile error**
demonstrates per-pass failure: the manager shows its diagnostic while other
editor/plugin work remains available. Clear the test checkbox to restore the
selected source, or reload the plugin.

The editor's Preview shaders setting also controls these passes. They affect the
shared preview image before egui overlays, including its docked/floating/fullscreen
views. Disable, project changes, expired frames or an empty pass list remove the
effect. They do not become chart shaders or apply to offline movie exports.

GLSL executes in the graphics driver and has that driver's limits; Wasm CPU fuel
does not bound shader compilation or GPU execution. Use trusted shader sources.
