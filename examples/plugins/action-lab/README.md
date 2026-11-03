# Editor action lab

Build the external Wasm module, then install this directory from Projects → Plugins:

```sh
cargo build --manifest-path examples/plugins/action-lab/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/action-lab/target/wasm32-unknown-unknown/release/phistudio_example_action_lab.wasm examples/plugins/action-lab/plugin.wasm
```

The panel can block native Delete or Undo, including the same actions requested
through `host/editor`. It can snap API seeks to four-beat bars while preserving
the action kind and caller. Buttons control playback, seek, invoke Undo and read
bounded status metadata without requesting a chart snapshot. Service errors are
shown in the panel and do not disable the plugin.

`host.action.before` is read-only. It may return Continue, Replace or Cancel, but
cannot control the editor recursively. Replacements retain document/cursor,
selection counts and source identity. Built-in handlers perform approved actions;
existing editor Tools remain built in. Native seek gestures and automatic
lifecycle/gesture cleanup are outside this hook.

Plugin windows are opened explicitly from Windows. Commands and converters are
available in the plugin's editor File submenu; the Projects loader only manages
installation, enable/disable, reload and removal.
