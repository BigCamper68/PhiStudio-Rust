# External note stamp

This standalone SDK-only Wasm plugin contributes a new tap tool. Existing editor
tools are unchanged. Select **Stamp note** in the toolbar or invoke **Select note
stamp** in the editor File submenu (`Alt+S`). A mouse click or touch tap on the note
grid adds one note using the host's exact snapped time and horizontal position.
A swipe yields to the editor's existing navigation.

```sh
cargo build --manifest-path examples/plugins/note-stamp/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/note-stamp/target/wasm32-unknown-unknown/release/phistudio_example_note_stamp.wasm examples/plugins/note-stamp/plugin.wasm
```

Install the directory from **Projects → Plugins** and enable it. The gesture uses
a metadata context and a bounded `host/document` inspection instead of copying
the chart. It appends to the source note array, creating it if absent or null.
One completed tap produces one transaction in the editor's Undo/Redo history.
Cancel, a second finger, project changes or disabling the plugin revoke pending
callbacks and their service continuations before they can edit the chart.

The compiled headless editor check exercises the actual toolbar, mouse/touch
routing, asynchronous document query, source preservation and Undo after unload:

```sh
cargo test -p phistudio-app --lib compiled_wasm_stamp --locked -- --ignored
```
