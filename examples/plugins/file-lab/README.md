# File workflow lab

An external SDK-only Wasm plugin that intercepts explicit native open, save and
export actions. It requests Hooks and UI, without document read/edit or file
access. All controls start unchecked.

```sh
cargo build --manifest-path examples/plugins/file-lab/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/file-lab/target/wasm32-unknown-unknown/release/phistudio_example_file_lab.wasm examples/plugins/file-lab/plugin.wasm
```

Install this directory in **Projects → Plugins** and enable **File workflow lab**.
Its **File workflow controls** panel has four checkboxes:

- **Block chart saves** cancels explicit chart and private-project saves.
- **Block chart opens** cancels chart opening and project/archive imports.
- **Block project exports** cancels native project/history export before the
  destination dialog. Preparing the private archive may already have finished.
- **Suggest a reviewed export name** suggests `reviewed-project.pez` for native
  project exports. The user still chooses the destination normally.

Toggle a blocker off to restore the normal action. The plugin receives
`FileIntercept` metadata through `host.file.before`, without OS paths, chart
snapshots or native callbacks. Cancellation leaves unsaved edits/history intact;
it also runs before Discard-and-open clears recovery. Recovery checkpoints are
not intercepted. Name replacement changes a suggested export basename, not the
selected save/open path or archive contents. Other replacements fail validation.

The compiled headless editor test clicks these actual checkboxes, checks blocked
and permitted Save/Open and verifies the renamed native export:

```sh
cargo test -p phistudio-app --lib compiled_wasm_file_lab --locked -- --ignored
```

Plugin windows are opened explicitly from Windows. Commands and converters are
available in the plugin's editor File submenu; the Projects loader only manages
installation, enable/disable, reload and removal.
