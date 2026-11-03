# Project pack

An SDK-only Wasm format provider for a project and its binary resources. It uses
`bridge: "project"`, requests Formats/Services/DocumentRead/ProjectRead and has no
ZIP, renderer, filesystem or platform dependency. The host supplies a native
project snapshot and streams entries through private numeric handles.

```sh
cargo build --manifest-path examples/plugins/project-pack/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/project-pack/target/wasm32-unknown-unknown/release/phistudio_example_project_pack.wasm examples/plugins/project-pack/plugin.wasm
```

Install this directory in **Projects → Plugins**, enable **Project pack**, then use its File submenu to
use **Export with plugin** with a `.pspack` destination. The native snapshot
contains the current unsaved chart, project options and assets. Export leaves
the saved project and editor dirty state unchanged. **Import with plugin** reads
that custom container, creates native package entries and asks the host to pack
them. The existing project importer validates the result and uses its normal
unsaved-changes/multiple-chart workflow. Android/iOS use system file pickers.

The example retains one operation at a time. Its format is little-endian:

1. The exact UTF-8 bytes `PhiStudio project 1\n`, then a `u32` file count.
2. For each file: `u32` UTF-8 path length, `u64` data length, path bytes, data bytes.

The guest pages input metadata, opens one archive member at a time, reads/writes
at most 256 KiB per request and closes read handles after use. Decode creates
named output files, streams bytes to them and calls `pack`, then completes with
`project.pez`. Correlations, offsets, sizes, paths and write acknowledgements are
checked. Malformed containers use `Fail` and leave the module enabled.

This intentionally uncompressed example is not a production compression format:
the custom input/output file still has a 256 MiB limit, even when a compressed
native project could expand to more. Paths/entry counts and expanded totals keep
the SDK's package limits. A codec needing compression can stream it separately.

The first compiled headless UI check transfers 300 additional files, a resource
larger than 16 MiB, music, an image, unknown source JSON and unsaved chart edits.
It uses the editor menu actions and unsaved-changes dialog, disables the
module and verifies ordinary native project export and independent editor Undo.
The second rejects escaping, duplicate, truncated and trailing entries while
preserving the current document/history:

```sh
cargo test -p phistudio-app --lib compiled_wasm_project_pack --locked -- --ignored --nocapture
```
