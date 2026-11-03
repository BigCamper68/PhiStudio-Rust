# Asset view

An independent Wasm plugin using `host/assets` v1. Its procedural `mark.bmp` is
part of this GPL-3.0-only example. The editor's Tools remain built in.

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release --locked
cp target/wasm32-unknown-unknown/release/phistudio_example_asset_view.wasm plugin.wasm
```

Install this directory in **Plugins**, enable **Asset view**, then use its window:

- The bundled image appears in the canvas, preview overlay and image track.
- **Read project resources** lists the current project's root, 32 entries per page.
- Enter a relative image path and choose **Show project image** to prepare a private
  snapshot. PNG, JPEG, WebP and BMP use the host's bounded decoder.
- **Use bundled image** switches back without reloading it.

`Services` grants access to this plugin's package. `ProjectRead` additionally grants
read-only access to the current project's files. No operating-system paths, live
files, guest filesystem imports or document copies cross the protocol. Ordinary
chart edits retain snapshots; resource edits, project replacement and plugin
unloading revoke them. Reopen externally modified files to get a new snapshot.
