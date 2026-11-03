# Portable note-shift plugin

This standalone crate depends only on the SDK and JSON serialization. It exposes
a command and a panel through the same protocol used by statically linked plugins.

From the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo build --manifest-path examples/plugins/note-shift/Cargo.toml --target wasm32-unknown-unknown --release
cp examples/plugins/note-shift/target/wasm32-unknown-unknown/release/phistudio_example_note_shift.wasm examples/plugins/note-shift/plugin.wasm
cargo run -p phistudio-plugins --example plugin-probe -- examples/plugins/note-shift assets/charts/phistudio-demo.rpe.json shift '{"offset":25}' /tmp/plugin-result.json
```

The probe loads the actual Wasm module, invokes its command, applies its changes,
unloads it, verifies Undo/Redo and saves a separate output chart. It explicitly
grants the manifest's requested capabilities. The example does not import OS or
editor functions into Wasm.

Open **Plugins** on Projects home.
On desktop install this directory; on mobile select a ZIP containing
`manifest.json` and `plugin.wasm` at its root. Enable the plugin, then use its
panel from Windows, or its command from the editor File submenu. Disabling it
removes the panel while existing Undo/Redo entries remain usable. The File
submenu also exposes the manifest's offset setting.

After building the module, exercise the actual contributed panel through egui
without requiring a native graphics window:

```sh
cargo test -p phistudio-app --lib compiled_wasm_package_installs_enables_and_edits_through_its_actual_panel --locked -- --ignored
```

This example opts into whole-chart action snapshots. For bounded reads and
service calls, see the neighboring `note-statistics` example. Remaining SDK
integration is tracked in `docs/PLUGINS.md`.

## Native package

The same source exports `export_native_plugin!` in native builds. Build on the
editor's target platform and package its cdylib:

```sh
cargo build --manifest-path examples/plugins/note-shift/Cargo.toml --release --locked
python3 examples/plugins/note-shift/package-native.py
cargo run -p phistudio-plugins --locked --example plugin-probe -- examples/plugins/note-shift/dist/native assets/charts/phistudio-demo.rpe.json shift '{"offset":25}' /tmp/native-plugin-result.json
cargo test -p phistudio-app --lib compiled_native_ --locked -- --ignored
```

Install `dist/native` through the manager. Its separate ID allows it to coexist
with the Wasm example. An optional path argument to `package-native.py` selects
a cross-compiled library. The C ABI is stable independently of Rust compiler
layout, but native libraries must match the editor's OS/architecture and have
full process access. This example uses `panic = "abort"`; use an unwinding
native build if relying on the SDK's Rust panic conversion.
