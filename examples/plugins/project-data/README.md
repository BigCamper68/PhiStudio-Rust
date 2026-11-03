# Project data

An SDK-only Wasm example storing a versioned tag in the chart's `phiStudioPlugins`
namespace. The tag is project data, separate from installation settings/storage.

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release --locked
cp target/wasm32-unknown-unknown/release/phistudio_example_project_data.wasm plugin.wasm
```

Install and enable this directory in **Plugins**. Enter a tag, choose its export
behavior and press **Store tag in project**. Undo/Redo works after unloading the
plugin. **Remove project tag** explicitly removes its entry. Opening another
project or re-enabling/reloading the plugin reads that project's own value.

Use a format provider's export dialog to choose **Preserve all plugin data**
(default), **Runtime data only**, or **Remove all plugin data**. Runtime export
omits editor-only tags and rejects entries marked Requires conversion until a
converter handles them. Removing all data is an explicit export-copy choice.
Native Save, recovery, history and ordinary native export preserve source data.

No existing editor Tool is moved into this example. It uses a bounded
`host/document.plugin_data` query and a `PatchOperation::PluginData` inside the
normal versioned transaction/history boundary.
