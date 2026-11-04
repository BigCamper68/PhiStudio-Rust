# SDK Lab

An external Wasm example using only `phistudio-sdk` and JSON. It exercises feature
discovery, tabs/tables/trees, explicit dialogs, note/line context commands, an
embedded timeline column, decoded music metadata, and a bounded HTTPS request.
It does not change the chart. It requires PhiStudio Rust 1.32.0 or newer.

```sh
cargo build --manifest-path examples/plugins/sdk-lab/Cargo.toml \
  --target wasm32-unknown-unknown --release --locked
cp examples/plugins/sdk-lab/target/wasm32-unknown-unknown/release/phistudio_example_sdk_lab.wasm \
  examples/plugins/sdk-lab/plugin.wasm
cargo run -p phistudio-plugins --bin phistudio-plugin -- \
  pack examples/plugins/sdk-lab artifacts/plugins/sdk-lab-0.1.0.zip
```

Install in Projects → Plugins and review permissions. Open File → SDK Lab;
open Windows → SDK timeline column separately. HTTPS runs only when its button
is pressed; it contacts `https://example.com`, with no project data or credentials.
Denying Audio or Network disables the corresponding button. UI and Timeline are
required to activate this particular demonstration. The tree lists host service
versions. The properties menu sends versioned source IDs to the report.
