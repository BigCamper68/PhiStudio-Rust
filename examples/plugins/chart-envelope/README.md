# Chart envelope

An external SDK-only Wasm format provider. It wraps canonical RPE bytes in
`PhiStudio chart 1\n` and reverses that transformation on import. This is a small
codec example, not a replacement for the editor's existing format converters.
It preserves the JSON bytes and unknown fields, but carries no project assets.

```sh
cargo build --manifest-path examples/plugins/chart-envelope/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/chart-envelope/target/wasm32-unknown-unknown/release/phistudio_example_chart_envelope.wasm examples/plugins/chart-envelope/plugin.wasm
```

Install this directory from **Projects → Plugins** and enable **Chart envelope**.
Use **Export with plugin**, choose a destination ending in `.pschart` and export.
**Import with plugin** decodes that file into a new private project through the
existing chart loader. Imports also work from **Projects → Plugins**. Unsaved
editor changes use the normal pending-open dialog. Android/iOS use system file
pickers; desktop accepts paths in the provider dialog.

The guest receives private input/output handles and uses `host/files` version 1
to read/write chunks of at most 256 KiB. Export requires DocumentRead; files stay
off the JSON message path except for individual base64 chunks. The example
correlates read/write responses and checks offsets, total size and output
acknowledgements. Normal invalid-file errors use `Fail`, leaving the plugin enabled.
This example retains one operation at a time; a concurrent driver needs a map
indexed by operation/correlation ID.

The compiled editor test clicks the editor menu controls, exports and imports,
preserves unknown chart data, disables the module and saves ordinary RPE:

```sh
cargo test -p phistudio-app --lib compiled_wasm_envelope --locked -- --ignored
```

Plugin windows are opened explicitly from Windows. Commands and converters are
available in the plugin's editor File submenu; the Projects loader only manages
installation, enable/disable, reload and removal.
