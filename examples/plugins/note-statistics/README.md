# Portable note statistics and service example

This extension reads the active line in bounded pages through `host/document`
and calls its own versioned `histogram` service for each page. Its panel shows
the result. No whole-chart snapshot, document edit, OS import or editor library
is needed. A changed project/revision aborts the scan with a normal error.

Build from the repository root:

```sh
cargo build --manifest-path examples/plugins/note-statistics/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/note-statistics/target/wasm32-unknown-unknown/release/phistudio_example_note_statistics.wasm examples/plugins/note-statistics/plugin.wasm
```

Install the directory (or a ZIP of its contents) from the plugin manager, enable
it, open its panel from Windows and press **Count notes on active line**. The editor's existing Tools are
independent of this extension.

Another plugin can reuse the service by declaring a dependency on
`example.note-statistics` version `^0.1` and requesting service `histogram`
version `^1`, method `count`, with a source-note array as arguments. A successful
result contains `counts`: `[other, tap, hold, flick, drag]`.

After building both example modules, run their actual editor panels headlessly:

```sh
cargo test -p phistudio-app --lib compiled_wasm_ --locked -- --ignored
```
