# Editor guides

This SDK-only extension contributes a passive preview overlay, a time-synchronized
track window and a static panel canvas. It uses theme tokens, normalized preview
coordinates and absolute timeline beats. Its settings checkbox hides all dynamic
guides without touching chart data or the editor's built-in tools.

```sh
cargo build --manifest-path examples/plugins/editor-guides/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/editor-guides/target/wasm32-unknown-unknown/release/phistudio_example_editor_guides.wasm examples/plugins/editor-guides/plugin.wasm
```

Install the directory (or a ZIP containing its manifest and module) from the
Projects plugin manager and enable it. Open the track explicitly from Windows;
it follows the editor's cursor/zoom. Close it normally or with Back and reopen
it from Windows.
The preview overlay is passive and leaves existing preview input routing intact.

Render callbacks are short, asynchronous and cannot edit documents. Prepare
expensive data through commands or services, cache it in the plugin, then render
that cache. Echo `frame.token` in a single Draw effect with all layers for that
frame. The host clips geometry and discards outdated ownership/document/layout
results. Marker sizes, stroke widths and font sizes always use screen points.
