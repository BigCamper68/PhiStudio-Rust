# Pixel grid

An external SDK-only Wasm resource provider that converts a text pixel grid to
a 24-bit BMP. The editor validates the image and imports it through its existing
project-resource workflow, including native resource Undo and project export.
No document-read or edit capability is requested.

```sh
cargo build --manifest-path examples/plugins/pixel-grid/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/pixel-grid/target/wasm32-unknown-unknown/release/phistudio_example_pixel_grid.wasm examples/plugins/pixel-grid/plugin.wasm
```

Open a private project from **Projects**, install this directory in
**Projects → Plugins**, enable **Pixel grid**, then choose **File → Pixel grid → Convert resource**.
Import [`sample.pgrid`](sample.pgrid). Desktop accepts the source path;
Android/iOS use the system file picker. The imported BMP appears in the normal
project resource library. It remains usable and can be undone after disabling
the module.

The text starts with `PSTILE width height`, followed by exactly `width * height`
six-digit RGB hex values in top-to-bottom row order. Both dimensions must be
1–64. These are example-format limits. The plugin writes the BMP through private
`host/files` handles and completes with `pixel-grid.bmp`; it receives no OS paths.
Malformed grids return `Fail` without failing the plugin instance.

```text
PSTILE 2 2
ff0000 00ff00
0000ff ffffff
```

The compiled editor test imports the actual guest's image, verifies its pixels,
exports/reimports a project and performs resource Undo after disable:

```sh
cargo test -p phistudio-app --lib compiled_wasm_pixel_grid --locked -- --ignored
```
