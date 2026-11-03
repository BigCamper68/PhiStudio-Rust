# Chart Linter

An external Wasm plugin for PhiStudio-Rust. Its rules follow the built-in chart
diagnostics in rpe-next; the functional reference is pinned in [REFERENCE.md](REFERENCE.md).
It uses the public SDK and does not replace any built-in editor tools.

## Install and use

Use the PhiStudio build supplied with this plugin: it includes the document
projection, project metadata, object navigation and native inspection services.
Earlier builds of the draft API may not recognize the `long_running` capability.

Open **Projects → Plugins → Install plugin**, choose `chart-linter-0.1.1.zip`,
and enable **Chart Linter**. In the editor, open **File → Chart Linter**.
Enable and Reload keep plugin windows closed. The loader only installs,
enables/disables, reloads and removes plugins.

- **Basic scan** checks notes, event tracks, BPM, parent chains, distance
  controls, initial track values, easing, music bounds, resources, Noise Fields,
  shader metadata and compatibility with RPE/Phira.
- **Deep scan** also checks native note positions, zero width, consecutive
  visibility over the final 300 ms, Flick/Drag-to-Tap timing and indirect
  JSON/GLSL resource references. It runs explicitly and can be canceled.
- Filter results by severity, object type and text; informational results can
  be hidden. Each page shows 20 issues. Totals include issues beyond the retained
  report limit.
- **Locate** selects the source object, switches its line/track and seeks near
  its beat. **Remove…** asks for confirmation and creates one ordinary editor
  Undo step. Scanning does not change the document or its history.

Chart edits, project replacement, asset changes, preview configuration changes
and plugin reloads invalidate results. Selection and cursor changes alone do not
invalidate a report. Late replies after cancellation are ignored; open resource
handles are closed.

## Engine differences and limits

Deep geometry uses the original parsed prpr animations, transforms, distance
controls and visibility, before arbitrary shader post-processing. It does not
execute rpe-next's JavaScript validators or spatial-camera mod. Foreign global
schemas and missing/version-mismatched providers are reported. Custom curve
metadata is retained and reported; the native engine determines playback.
Original prpr currently ignores `bpmfactor`, so timing checks follow its playback
and report non-default factors. Missing audio has no invented duration limit.

The UI is currently English. GPU validation with Mesa does not replace checks
on a physical Android device.

Work is bounded: 40,000 service queries, 500,000 note/event objects, 20,000 lines,
1,024 event layers per line, 10,000 BPM entries, 20,000 resource entries/references
with 8 MiB path/reference budgets, and 4 MiB per indirect resource. The report
retains at most 10,000 rows / 8 MiB. Native queries cap parent depth at 64 and
adapt their sample batches to the host's work budget. Limits or unavailable
checks produce an explicitly incomplete report.

## Build and verify

From the repository root, with `wasm32-unknown-unknown` installed:

```sh
cargo test --manifest-path plugins/chart-linter/Cargo.toml --locked
cargo clippy --manifest-path plugins/chart-linter/Cargo.toml --all-targets --locked -- -D warnings
python3 plugins/chart-linter/package.py
cargo test -p phistudio-app --lib compiled_chart_linter --locked -- --ignored
cargo build -p phistudio-app --example chart-linter-probe --locked
xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/examples/chart-linter-probe artifacts/checks/chart-linter
```

The packager writes `plugin.wasm` and `artifacts/plugins/chart-linter-0.1.1.zip`.
The integration checks use the installed module, actual egui panel, scoped host
services, real project files, source navigation, cancellation and editor Undo.
The graphics probe compares inspection coordinates with original GPU picking,
including below-line notes and parent transforms, and exercises the compiled
Wasm deep scan against the native preview.

Source: `src/lib.rs` owns plugin lifecycle/actions, `scan.rs` the paginated
pipeline, `scan/auxiliary.rs` extension metadata, `lint.rs` diagnostic rules,
`resources.rs` resource resolution and `ui.rs` the declarative panel.

Licensed under GPL-3.0-only, as in the repository's `LICENSE`. No rpe-next code
or assets are included.
