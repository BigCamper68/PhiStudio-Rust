# Service interception lab

This SDK-only external Wasm plugin demonstrates a virtual `host/greeting` service.
The editor has no built-in greeting provider. The plugin's `host.service.before`
hook redirects it to the plugin's own independently versioned `greeting` service.
The caller still receives its original correlation ID and `host/greeting` address.

```sh
cargo build --manifest-path examples/plugins/service-lab/Cargo.toml --target wasm32-unknown-unknown --release --locked
cp examples/plugins/service-lab/target/wasm32-unknown-unknown/release/phistudio_example_service_lab.wasm examples/plugins/service-lab/plugin.wasm
```

Install the directory from **Projects → Plugins**, enable it and open its
**Service interception** panel. **Call virtual greeting** displays
`host/greeting: Hello, PhiStudio`. Enable **Block the virtual service** and call
again to receive a normal `hook_cancelled` service error. Uncheck it to restore
the replacement. The plugin does not request document read/edit access.

Only calls to the virtual greeting are replaced; other service calls continue.
A replacement provider is authorized by the replacing hook's Services capability
and declared dependencies. Calling the plugin's own service needs no dependency.
Hooks run in ascending priority/plugin-ID order. Cancellation stops the chain.
Hook side effects and their service continuations cannot edit charts or recursively
enter service interceptors. Disabling an involved module revokes queued results.

The compiled headless editor check clicks the actual panel button and checkbox,
checks the replacement/cancellation/re-enable path and leaves the chart untouched:

```sh
cargo test -p phistudio-app --lib compiled_wasm_service_lab --locked -- --ignored
```

Plugin windows are opened explicitly from Windows. Commands and converters are
available in the plugin's editor File submenu; the Projects loader only manages
installation, enable/disable, reload and removal.
