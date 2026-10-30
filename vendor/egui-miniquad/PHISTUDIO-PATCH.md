Source: crates.io `egui-miniquad` 0.14.0 (MIT OR Apache-2.0).

Compatibility change in `src/painter.rs`: pass
`miniquad::TextureFormat::RGBA8` to `Texture::from_raw_id`, as required by
Phira's pinned Miniquad fork. `src/lib.rs` adds `set_pixels_per_point` so the
host can supply Android display density before the first frame. It also adds
`push_input_event`: PhiStudio sends all ordered touch events, including second
fingers and cancellations, alongside the primary UI pointer. The painter also replaces growing vertex/index buffers before retiring the old
allocations, and deletes them only after applying the new bindings. Miniquad's
cached vertex attributes must not see an immediately reused OpenGL buffer name;
the old order caused a driver SIGSEGV on dense event grids. Missing textures are
resolved before buffer allocation so skipped meshes cannot leak retired buffers.
The DPI helpers in `src/dpi.rs` resolve pending RawInput scale for both screen
bounds and pointer coordinates. Scaling is queued directly as RawInput, avoiding
Context's deferred setter and its previous-frame screen rectangle. The host can
query this effective scale for Android touch routing. Native DPI updates do not
replace an explicit host scale. The pure DPI path is included in the app's
workspace tests, covering first-frame bounds, density changes and button input.
All other source is unchanged.

This adapter is pinned with egui 0.21 because prpr uses Miniquad 0.3.15.
Upgrading to a modern Miniquad/egui backend requires coordinating with prpr;
adding a second Macroquad context is not a supported migration path.
